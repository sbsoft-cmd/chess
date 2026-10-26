//! Лобби: матчмейкинг для duel и team_2v2.

use chess_logic::{GameState, Seat};
use chess_protocol::ServerMessage;
use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, oneshot};
use tokio::time::interval;
use tracing::info;

use crate::session::{self, SessionEvent};

const INVITE_TIMEOUT: Duration = Duration::from_secs(10);
const INVITE_TICK: Duration = Duration::from_millis(500);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Mode {
    Duel,
    Team2v2Random,
    Team2v2Pick,
    Team2v2Explicit,
    Classic8x8,
    Classic8x8Freestyle,
    DuelFreestyle,
}

impl Mode {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "duel" => Some(Mode::Duel),
            "duel_freestyle" => Some(Mode::DuelFreestyle),
            "team_2v2_random" => Some(Mode::Team2v2Random),
            "team_2v2_pick" => Some(Mode::Team2v2Pick),
            "team_2v2_explicit" => Some(Mode::Team2v2Explicit),
            "classic_8x8" => Some(Mode::Classic8x8),
            "classic_8x8_freestyle" => Some(Mode::Classic8x8Freestyle),
            _ => None,
        }
    }

    pub fn is_duel(self) -> bool {
        matches!(
            self,
            Mode::Duel | Mode::DuelFreestyle | Mode::Classic8x8 | Mode::Classic8x8Freestyle
        )
    }
}

type Key = (String, String, Mode); // (variant, time_control, mode)

#[derive(Debug, Clone, PartialEq, Eq)]
enum Status {
    Free,
    Inviting { to: u64 },
    InvitedBy { from: u64 },
    InPair { pair_id: u64 },
}

struct Client {
    id: u64,
    name: String,
    variant: String,
    time_control: String,
    mode: Mode,
    wants_opponent: Option<String>,          // для 1×1
    wants_partner: Option<String>,           // для team_2v2_pick / explicit
    wants_opponents: Option<[String; 2]>,    // для explicit
    status: Status,
    tx: mpsc::UnboundedSender<ServerMessage>,
    rx: Option<mpsc::UnboundedReceiver<SessionEvent>>,
}

struct Pair {
    id: u64,
    first: u64,
    second: u64,
    key: Key,
}

#[derive(Clone)]
struct Invite {
    from: u64,
    to: u64,
    deadline: Instant,
}

pub enum LobbyMsg {
    Register {
        name: String,
        variant: String,
        time_control: String,
        mode: String,
        opponent_name: Option<String>,
        partner_name: Option<String>,        // <-- новое
        to_session_rx: mpsc::UnboundedReceiver<SessionEvent>,
        from_session_tx: mpsc::UnboundedSender<ServerMessage>,
        reply_tx: oneshot::Sender<RegisterReply>,
    },
    ClientGone { id: u64 },
    Invite { from: u64, target_name: String },
    CancelInvite { from: u64 },
    AcceptInvite { from: u64, inviter_name: String },
    DeclineInvite { from: u64, inviter_name: String },
    LeaveLobby { id: u64 },
}

pub enum RegisterReply {
    Ok(u64),
    NameTaken,
}

pub async fn lobby_task(mut rx: mpsc::UnboundedReceiver<LobbyMsg>) {
    let mut pending_invites: Vec<(u64, String)> = Vec::new();
    let mut clients: HashMap<u64, Client> = HashMap::new();
    let mut names: HashMap<String, u64> = HashMap::new();
    let mut queues: HashMap<Key, VecDeque<u64>> = HashMap::new();
    let mut pairs: HashMap<u64, Pair> = HashMap::new();
    let mut pair_pool: HashMap<Key, VecDeque<u64>> = HashMap::new();
    let mut invites: HashMap<u64, Invite> = HashMap::new();

    let mut next_client_id: u64 = 0;
    let mut next_pair_id: u64 = 0;
    let mut next_game_id: u64 = 0;
    let mut ticker = interval(INVITE_TICK);

    loop {
        tokio::select! {
            _ = ticker.tick() => {
                let now = Instant::now();
                let expired: Vec<u64> = invites.iter()
                .filter(|(_, inv)| now >= inv.deadline)
                .map(|(from, _)| *from)
                .collect();
                for from in expired {
                    if let Some(inv) = invites.remove(&from) {
                        handle_invite_timeout(&mut clients, from, inv.to);
                    }
                }

                // ============================================================
                // Кросс-сведение: Pick-пара + 2 одиночки из Random.
                // ============================================================
                let pick_keys: Vec<Key> = pair_pool
                .iter()
                .filter(|(k, _)| k.2 == Mode::Team2v2Pick)
                .map(|(k, _)| k.clone())
                .collect();

                for pick_key in pick_keys {
                    let random_key = (pick_key.0.clone(), pick_key.1.clone(), Mode::Team2v2Random);
                    try_start_cross(
                        &pick_key,
                        &random_key,
                        &mut clients,
                        &mut pairs,
                        &mut pair_pool,
                        &mut queues,
                        &mut next_game_id,
                    );
                }

            }
            msg = rx.recv() => {
                let msg = match msg { Some(m) => m, None => return };
                match msg {
                    LobbyMsg::Register {
                        name,
                        variant,
                        time_control,
                        mode,
                        opponent_name,
                        partner_name,
                        to_session_rx,
                        from_session_tx,
                        reply_tx,
                    } => {
                        let lname = name.to_lowercase();

                        // Уникальность имени: если старое соединение мертво — удаляем его.
                        if let Some(&old_id) = names.get(&lname) {
                            let old_alive = clients.get(&old_id).map_or(false, |c| !c.tx.is_closed());
                            if old_alive {
                                let _ = reply_tx.send(RegisterReply::NameTaken);
                                let _ = from_session_tx.send(ServerMessage::Error {
                                    message: "имя занято".into(),
                                    code: Some("name_taken".into()),
                                });
                                continue;
                            } else {
                                info!(
                                    "[lobby] имя '{}' было занято мертвым клиентом {}, удаляю",
                                    name, old_id
                                );
                                remove_client(
                                    old_id,
                                    &mut clients,
                                    &mut names,
                                    &mut invites,
                                    &mut queues,
                                    &mut pairs,
                                    &mut pair_pool,
                                );
                            }
                        }

                        let mode = match Mode::parse(&mode) {
                            Some(m) => m,
                            None => {
                                let _ = reply_tx.send(RegisterReply::NameTaken);
                                let _ = from_session_tx.send(ServerMessage::Error {
                                    message: format!("неизвестный режим: {mode}"),
                                                             code: Some("bad_mode".into()),
                                });
                                continue;
                            }
                        };

                        // --- Разбор "хотелок" из Hello ---
                        let wants_partner: Option<String> = partner_name
                        .clone()
                        .map(|s| s.trim().to_lowercase())
                        .filter(|s| !s.is_empty());

                        let wants_opponents: Option<[String; 2]> = opponent_name
                        .as_deref()
                        .and_then(parse_opponents);

                        // Для team_2v2_explicit обязательны оба поля.
                        if mode == Mode::Team2v2Explicit
                            && (wants_partner.is_none() || wants_opponents.is_none())
                            {
                                let _ = reply_tx.send(RegisterReply::NameTaken);
                                let _ = from_session_tx.send(ServerMessage::Error {
                                    message:
                                    "team_2v2_explicit требует partner_name и opponent_name (два имени через запятую)"
                                    .into(),
                                                             code: Some("bad_explicit".into()),
                                });
                                continue;
                            }

                            next_client_id += 1;
                            let id = next_client_id;
                            names.insert(lname, id);
                            let _ = reply_tx.send(RegisterReply::Ok(id));

                            clients.insert(
                                id,
                                Client {
                                    id,
                                    name: name.clone(),
                                           variant: variant.clone(),
                                           time_control: time_control.clone(),
                                           mode,
                                           wants_opponent: opponent_name.clone().map(|s| s.to_lowercase()),
                                           wants_partner,
                                           wants_opponents,
                                           status: Status::Free,
                                           tx: from_session_tx,
                                           rx: Some(to_session_rx),
                                },
                            );

                            info!(
                                "[lobby] client {id} ({name}) зарегистрирован mode={:?} opponent={:?} partner={:?}",
                                  mode, opponent_name, partner_name
                            );

                            // --- Отложенные приглашения (гонка "Invite до Register") ---
                            let deferred: Vec<(u64, String)> = pending_invites
                            .iter()
                            .filter(|(from, _)| *from == id)
                            .cloned()
                            .collect();
                            pending_invites.retain(|(from, _)| *from != id);
                            for (from, target_name) in deferred {
                                info!(
                                    "[lobby] обработка отложенного Invite from={from}, target={target_name}"
                                );
                                handle_invite(
                                    from,
                                    &target_name,
                                    &mut clients,
                                    &names,
                                    &mut invites,
                                    &mut pairs,
                                    &mut pair_pool,
                                    &mut next_pair_id,
                                    &mut next_game_id,
                                );
                            }

                            // --- Маршрутизация по режиму ---
                            if mode.is_duel() {
                                try_match_duel_named(
                                    id,
                                    &mut clients,
                                    &mut queues,
                                    &mut names,
                                    &mut next_game_id,
                                );
                            } else {
                                match mode {
                                    Mode::Team2v2Random => {
                                        let key = (variant.clone(), time_control.clone(), mode);
                                        let q = queues.entry(key.clone()).or_default();
                                        q.push_back(id);
                                        info!(
                                            "[lobby] client {id} ({name}) в team_2v2_random, очередь {}",
                                              q.len()
                                        );
                                        try_match_random(
                                            &key,
                                            &mut clients,
                                            &mut queues,
                                            &mut pairs,
                                            &mut pair_pool,
                                            &mut next_pair_id,
                                            &mut next_game_id,
                                        );
                                    }
                                    Mode::Team2v2Pick => {
                                        info!("[lobby] client {id} ({name}) в team_2v2_pick, ожидает");
                                    }
                                    Mode::Team2v2Explicit => {
                                        info!(
                                            "[lobby] client {id} ({name}) в team_2v2_explicit, партнёр={:?}, соперники={:?}",
                                              partner_name, opponent_name
                                        );
                                        try_match_explicit(id, &mut clients, &names, &mut next_game_id);
                                    }
                                    _ => {}
                                }
                            }
                    }
                    LobbyMsg::ClientGone { id } => {
                        remove_client(id, &mut clients, &mut names, &mut invites, &mut queues, &mut pairs, &mut pair_pool);
                    }
                    LobbyMsg::Invite { from, target_name } => {
                        if !clients.contains_key(&from) {
                            info!("[lobby] Invite от незарегистрированного {from} — буферизуем");
                            pending_invites.push((from, target_name));
                            continue;
                        }
                        info!("[lobby] Invite from={from}, target={target_name}");
                        handle_invite(
                            from,
                            &target_name,
                            &mut clients,
                            &names,
                            &mut invites,
                            &mut pairs,
                            &mut pair_pool,
                            &mut next_pair_id,
                            &mut next_game_id,
                        );
                    }
                    LobbyMsg::CancelInvite { from } => {
                        if let Some(inv) = invites.remove(&from) {
                            if let Some(c) = clients.get_mut(&from) { c.status = Status::Free; }
                            let from_name = clients.get(&from).map(|c| c.name.clone()).unwrap_or_else(|| "?".into());
                            if let Some(target) = clients.get_mut(&inv.to) {
                                target.status = Status::Free;
                                let _ = target.tx.send(ServerMessage::InviteCancelled { from_name });
                            }
                        }
                    }
                    LobbyMsg::AcceptInvite { from, inviter_name } => {
                        handle_accept(from, &inviter_name, &mut clients, &names, &mut invites, &mut pairs, &mut pair_pool, &mut next_pair_id, &mut next_game_id);
                    }
                    LobbyMsg::DeclineInvite { from, inviter_name } => {
                        let inviter_id = names.get(&inviter_name.to_lowercase()).copied();
                        if let Some(inviter_id) = inviter_id {
                            if let Some(inv) = invites.remove(&inviter_id) {
                                if inv.to == from {
                                    let by_name = clients.get(&from).map(|c| c.name.clone()).unwrap_or_else(|| "?".into());
                                    if let Some(inviter) = clients.get_mut(&inviter_id) {
                                        inviter.status = Status::Free;
                                        let _ = inviter.tx.send(ServerMessage::InviteDeclined { by_name });
                                    }
                                    if let Some(c) = clients.get_mut(&from) { c.status = Status::Free; }
                                }
                            }
                        }
                    }
                    LobbyMsg::LeaveLobby { id } => {
                        remove_from_queues(id, &mut queues, &mut pair_pool, &mut pairs);
                        if let Some(inv) = invites.remove(&id) {
                            let from_name = clients.get(&id).map(|c| c.name.clone()).unwrap_or_else(|| "?".into());
                            if let Some(target) = clients.get_mut(&inv.to) {
                                target.status = Status::Free;
                                let _ = target.tx.send(ServerMessage::InviteCancelled { from_name });
                            }
                        }
                        if let Some(c) = clients.get_mut(&id) { c.status = Status::Free; }
                    }
                }
            }
        }
    }
}

// ============================================================
// Duel (1×1) с поддержкой именованного соперника
// ============================================================

fn try_match_duel_named(
    id: u64,
    clients: &mut HashMap<u64, Client>,
    queues: &mut HashMap<Key, VecDeque<u64>>,
    names: &HashMap<String, u64>,
    next_game_id: &mut u64,
) {
    let (mode, key, wants, my_name) = {
        let c = match clients.get(&id) { Some(c) => c, None => return };
        if !c.mode.is_duel() { return; }
        let key = (c.variant.clone(), c.time_control.clone(), c.mode);
        (c.mode, key, c.wants_opponent.clone(), c.name.clone())
    };

    // 1. Если задан opponent_name, ищем его.
    if let Some(ref target_name) = wants {
        // Ищем в clients клиента с этим именем.
        if let Some(&target_id) = names.get(target_name) {
            if target_id != id {
                if let Some(target) = clients.get(&target_id) {
                    if target.status == Status::Free
                        && target.variant == key.0
                        && target.time_control == key.1
                        && target.mode == mode
                        {
                            // Соединяем.
                            create_duel_pair(id, target_id, clients, next_game_id);
                            return;
                        }
                }
            }
        }
    }

    // 2. Если я сам жду кого-то (wants) — ищу, нет ли в clients того,
    //    кто хочет меня (wants_opponent == Some(my_name)) и свободен.
    let my_lower = my_name.to_lowercase();
    let mut found_partner: Option<u64> = None;
    for (other_id, other) in clients.iter() {
        if *other_id == id { continue; }
        if other.status != Status::Free { continue; }
        if other.variant != key.0 || other.time_control != key.1 || other.mode != mode { continue; }
        if let Some(ref ow) = other.wants_opponent {
            if *ow == my_lower {
                found_partner = Some(*other_id);
                break;
            }
        }
    }
    if let Some(partner_id) = found_partner {
        create_duel_pair(id, partner_id, clients, next_game_id);
        return;
    }

    // 3. Если я хочу кого-то, и он ещё не появился — жду (в очереди).
    //    Если я не хочу никого (wants = None) — стандартный матчмейкинг.

    // 3. Если я хочу кого-то (wants = Some) — жду только его.
    //    Если wants = None — стандартный матчмейкинг (случайный).
    if let Some(ref target_name) = wants {
        // Ждём конкретного игрока: не соединяемся со случайными.
        info!("[lobby] client {id} ({my_name}) ждёт соперника '{target_name}' (не случайного)");
        // Добавляем в очередь "ожидающих именованного соперника".
        // Стандартный матчмейкинг не должен брать этого клиента.
        // Мы храним его в queues, но при стандартном матчмейкинге
        // пропускаем клиентов с wants_opponent != None.
        // Для этого стандартный матчмейкинг (для wants=None) должен
        // проверять wants_opponent у клиентов из очереди.
        queues.entry(key).or_default().push_back(id);
        return;
    }

    // Стандартный матчмейкинг (wants = None).
    let q = queues.entry(key.clone()).or_default();

    // Ищем партнёра, у которого wants_opponent = None (или Some(my_name)).
    let my_lower = my_name.to_lowercase();
    let mut partner_idx: Option<usize> = None;
    for (i, &partner_id) in q.iter().enumerate() {
        if partner_id == id { continue; }
        if let Some(partner) = clients.get(&partner_id) {
            if partner.status != Status::Free { continue; }
            if partner.variant != key.0 || partner.time_control != key.1 || partner.mode != mode { continue; }
            match &partner.wants_opponent {
                None => { partner_idx = Some(i); break; }
                Some(t) if *t == my_lower => { partner_idx = Some(i); break; }
                _ => continue,
            }
        }
    }

    if let Some(i) = partner_idx {
        let partner_id = q.remove(i).unwrap();
        create_duel_pair(id, partner_id, clients, next_game_id);
    } else {
        q.push_back(id);
    }

}

fn create_duel_pair(
    a_id: u64,
    b_id: u64,
    clients: &mut HashMap<u64, Client>,
    next_game_id: &mut u64,
) {
    let a = match clients.remove(&a_id) { Some(c) => c, None => return };
    let b = match clients.remove(&b_id) { Some(c) => c, None => { clients.insert(a_id, a); return; } };

    let (white, black) = if a.id < b.id { (a, b) } else { (b, a) };
    let white_rx = white.rx.unwrap();
    let black_rx = black.rx.unwrap();

    *next_game_id += 1;
    let game_id = format!("g{}", next_game_id);
    let (initial, increment) = parse_time_control(&white.time_control);

    let state = match white.mode {
        Mode::Classic8x8 => GameState::new_classic_8x8_with_clock(initial, increment),
        Mode::Classic8x8Freestyle => {
            GameState::new_classic_8x8_freestyle_with_clock(initial, increment)
        }
        Mode::DuelFreestyle => {
            let mut s = GameState::new_with_clock(initial, increment);
            chess_logic::apply_random_position(&mut s.board);
            s
        }
        _ => GameState::new_with_clock(initial, increment),
    };

    info!("[lobby] duel {game_id}: {} (w) vs {} (b), mode={:?}", white.name, black.name, white.mode);

    tokio::spawn(session::run_duel(
        game_id, state, white.name.clone(), black.name.clone(),
                                   white.tx, black.tx, white_rx, black_rx,
    ));
}

// ============================================================
// Team 2v2 Random
// ============================================================

fn try_match_random(
    key: &Key,
    clients: &mut HashMap<u64, Client>,
    random_queues: &mut HashMap<Key, VecDeque<u64>>,
    pairs: &mut HashMap<u64, Pair>,
    pair_pool: &mut HashMap<Key, VecDeque<u64>>,
    next_pair_id: &mut u64,
    next_game_id: &mut u64,
) {
    loop {
        let q = match random_queues.get_mut(key) { Some(q) => q, None => return };
        if q.len() < 4 { return; }
        let mut ids: Vec<u64> = Vec::with_capacity(4);
        for _ in 0..4 { match q.pop_front() { Some(id) => ids.push(id), None => break } }
        if ids.len() != 4 { for id in ids { q.push_front(id); } return; }

        let (a, b, c, d) = (ids[0], ids[1], ids[2], ids[3]);
        *next_pair_id += 1;
        let pair1 = *next_pair_id;
        *next_pair_id += 1;
        let pair2 = *next_pair_id;

        pairs.insert(pair1, Pair { id: pair1, first: a, second: b, key: key.clone() });
        pairs.insert(pair2, Pair { id: pair2, first: c, second: d, key: key.clone() });

        let a_name = clients.get(&a).map(|x| x.name.clone()).unwrap_or_default();
        let b_name = clients.get(&b).map(|x| x.name.clone()).unwrap_or_default();
        let c_name = clients.get(&c).map(|x| x.name.clone()).unwrap_or_default();
        let d_name = clients.get(&d).map(|x| x.name.clone()).unwrap_or_default();

        if let Some(cl) = clients.get_mut(&a) { cl.status = Status::InPair { pair_id: pair1 }; let _ = cl.tx.send(ServerMessage::TeamFormed { partner_name: b_name.clone(), you_are: Seat::First }); }
        if let Some(cl) = clients.get_mut(&b) { cl.status = Status::InPair { pair_id: pair1 }; let _ = cl.tx.send(ServerMessage::TeamFormed { partner_name: a_name.clone(), you_are: Seat::Second }); }
        if let Some(cl) = clients.get_mut(&c) { cl.status = Status::InPair { pair_id: pair2 }; let _ = cl.tx.send(ServerMessage::TeamFormed { partner_name: d_name.clone(), you_are: Seat::First }); }
        if let Some(cl) = clients.get_mut(&d) { cl.status = Status::InPair { pair_id: pair2 }; let _ = cl.tx.send(ServerMessage::TeamFormed { partner_name: c_name.clone(), you_are: Seat::Second }); }

        let pool = pair_pool.entry(key.clone()).or_default();
        pool.push_back(pair1);
        pool.push_back(pair2);
        info!("[lobby] team_2v2_random: собрано две пары {pair1} / {pair2}, в пуле {} пар", pool.len());
        try_start_pairs(key, clients, pairs, pair_pool, next_game_id);
    }
}

// ============================================================
// Пул пар
// ============================================================

fn try_start_pairs(
    key: &Key,
    clients: &mut HashMap<u64, Client>,
    pairs: &mut HashMap<u64, Pair>,
    pair_pool: &mut HashMap<Key, VecDeque<u64>>,
    next_game_id: &mut u64,
) {
    loop {
        let pool = match pair_pool.get_mut(key) { Some(p) => p, None => return };
        if pool.len() < 2 { return; }
        let p1_id = match pool.pop_front() { Some(x) => x, None => return };
        let p2_id = match pool.pop_front() { Some(x) => x, None => return };

        let p1 = match pairs.remove(&p1_id) { Some(p) => p, None => return };
        let p2 = match pairs.remove(&p2_id) { Some(p) => p, None => return };

        let white_first_id = p1.first;
        let white_second_id = p1.second;
        let black_first_id = p2.first;
        let black_second_id = p2.second;

        let mut white_first = match clients.remove(&white_first_id) { Some(c) => c, None => return };
        let mut white_second = match clients.remove(&white_second_id) { Some(c) => c, None => { clients.insert(white_first_id, white_first); return; } };
        let mut black_first = match clients.remove(&black_first_id) { Some(c) => c, None => { clients.insert(white_first_id, white_first); clients.insert(white_second_id, white_second); return; } };
        let mut black_second = match clients.remove(&black_second_id) { Some(c) => c, None => {
            clients.insert(white_first_id, white_first);
            clients.insert(white_second_id, white_second);
            clients.insert(black_first_id, black_first);
            return;
        } };

        *next_game_id += 1;
        let game_id = format!("g{}", next_game_id);
        let (initial, increment) = parse_time_control(&white_first.time_control);

        let state = GameState::new_team_2v2_with_clock(initial, increment);

        info!("[lobby] team_2v2 {game_id}: {} + {} vs {} + {}", white_first.name, white_second.name, black_first.name, black_second.name);

        tokio::spawn(session::run_team_2v2(
            game_id, state,
            vec![
                (white_first.name.clone(), white_first.tx, white_first.rx.take().unwrap(), Seat::First, chess_logic::Color::White),
                                           (white_second.name.clone(), white_second.tx, white_second.rx.take().unwrap(), Seat::Second, chess_logic::Color::White),
                                           (black_first.name.clone(), black_first.tx, black_first.rx.take().unwrap(), Seat::First, chess_logic::Color::Black),
                                           (black_second.name.clone(), black_second.tx, black_second.rx.take().unwrap(), Seat::Second, chess_logic::Color::Black),
            ],
        ));
    }
}

// ============================================================
// Приглашения (team_2v2_pick)
// ============================================================

fn handle_invite(
    from: u64,
    target_name: &str,
    clients: &mut HashMap<u64, Client>,
    names: &HashMap<String, u64>,
    invites: &mut HashMap<u64, Invite>,
    pairs: &mut HashMap<u64, Pair>,
    pair_pool: &mut HashMap<Key, VecDeque<u64>>,
    next_pair_id: &mut u64,
    next_game_id: &mut u64,
) {
    // --- 1. Данные приглашающего ---
    let (from_name, from_variant, from_tc, from_mode, from_free) = {
        let c = match clients.get(&from) {
            Some(c) => c,
            None => {
                info!("[lobby] Invite: клиент {from} не найден");
                return;
            }
        };
        (
            c.name.clone(),
         c.variant.clone(),
         c.time_control.clone(),
         c.mode,
         matches!(c.status, Status::Free),
        )
    };

    if from_mode != Mode::Team2v2Pick {
        if let Some(c) = clients.get_mut(&from) {
            let _ = c.tx.send(ServerMessage::InviteError {
                code: "wrong_mode".into(),
                              message: "приглашения доступны только в team_2v2_pick".into(),
            });
        }
        return;
    }

    // --- 2. Ищем цель по имени ---
    let target_id = match names.get(&target_name.to_lowercase()).copied() {
        Some(x) => x,
        None => {
            if let Some(c) = clients.get_mut(&from) {
                let _ = c.tx.send(ServerMessage::InviteError {
                    code: "not_found".into(),
                                  message: format!("игрок {target_name} не найден"),
                });
            }
            return;
        }
    };
    if target_id == from {
        if let Some(c) = clients.get_mut(&from) {
            let _ = c.tx.send(ServerMessage::InviteError {
                code: "self".into(),
                              message: "нельзя пригласить себя".into(),
            });
        }
        return;
    }

    let (t_name, t_variant, t_tc, t_mode, t_free) = {
        let t = match clients.get(&target_id) {
            Some(c) => c,
            None => {
                info!("[lobby] Invite: цель {target_id} не найдена в clients");
                return;
            }
        };
        (
            t.name.clone(),
         t.variant.clone(),
         t.time_control.clone(),
         t.mode,
         matches!(t.status, Status::Free),
        )
    };

    if t_mode != Mode::Team2v2Pick {
        if let Some(c) = clients.get_mut(&from) {
            let _ = c.tx.send(ServerMessage::InviteError {
                code: "wrong_mode".into(),
                              message: format!("{t_name} не в режиме выбора партнёра"),
            });
        }
        return;
    }
    if t_variant != from_variant || t_tc != from_tc {
        if let Some(c) = clients.get_mut(&from) {
            let _ = c.tx.send(ServerMessage::InviteError {
                code: "mismatch".into(),
                              message: format!(
                                  "у {t_name} другой variant/time_control ({} {} vs {} {})",
                                               t_variant, t_tc, from_variant, from_tc
                              ),
            });
        }
        return;
    }

    // --- 3. Встречное приглашение? ---
    let crossed = invites
    .get(&target_id)
    .map(|inv| inv.to == from)
    .unwrap_or(false);

    if crossed {
        info!(
            "[lobby] встречное приглашение: {from_name} ↔ {t_name}, создаём пару"
        );
        invites.remove(&target_id);
        invites.remove(&from);
        create_pair_from_invite(
            from,
            target_id,
            clients,
            pairs,
            pair_pool,
            next_pair_id,
            next_game_id,
        );
        return;
    }

    // --- 4. Проверка занятости ---
    if !from_free {
        if let Some(c) = clients.get_mut(&from) {
            let _ = c.tx.send(ServerMessage::InviteError {
                code: "busy".into(),
                              message: "вы уже заняты".into(),
            });
        }
        return;
    }
    if !t_free {
        if let Some(c) = clients.get_mut(&from) {
            let _ = c.tx.send(ServerMessage::InviteError {
                code: "target_busy".into(),
                              message: format!("{t_name} занят"),
            });
        }
        return;
    }

    // --- 5. Создаём приглашение ---
    let deadline = Instant::now() + INVITE_TIMEOUT;
    invites.insert(
        from,
        Invite {
            from,
            to: target_id,
            deadline,
        },
    );
    if let Some(c) = clients.get_mut(&from) {
        c.status = Status::Inviting { to: target_id };
        let _ = c.tx.send(ServerMessage::InviteSent {
            to_name: t_name.clone(),
        });
    }
    if let Some(t) = clients.get_mut(&target_id) {
        t.status = Status::InvitedBy { from };
        let _ = t.tx.send(ServerMessage::InviteReceived {
            from_name,
            variant: from_variant,
            time_control: from_tc,
        });
    }
}

/// Создать пару из двух игроков. Используется и встречным приглашением,
/// и обычным `AcceptInvite`.
fn create_pair_from_invite(
    first_id: u64,
    second_id: u64,
    clients: &mut HashMap<u64, Client>,
    pairs: &mut HashMap<u64, Pair>,
    pair_pool: &mut HashMap<Key, VecDeque<u64>>,
    next_pair_id: &mut u64,
    next_game_id: &mut u64,
) {
    let inviter_name = clients
    .get(&first_id)
    .map(|c| c.name.clone())
    .unwrap_or_default();
    let from_name = clients
    .get(&second_id)
    .map(|c| c.name.clone())
    .unwrap_or_default();
    let key = match clients.get(&first_id) {
        Some(c) => (c.variant.clone(), c.time_control.clone(), c.mode),
        None => return,
    };

    *next_pair_id += 1;
    let pair_id = *next_pair_id;
    pairs.insert(
        pair_id,
        Pair {
            id: pair_id,
            first: first_id,
            second: second_id,
            key: key.clone(),
        },
    );

    if let Some(c) = clients.get_mut(&first_id) {
        c.status = Status::InPair { pair_id };
        let _ = c.tx.send(ServerMessage::TeamFormed {
            partner_name: from_name.clone(),
                          you_are: Seat::First,
        });
    }
    if let Some(c) = clients.get_mut(&second_id) {
        c.status = Status::InPair { pair_id };
        let _ = c.tx.send(ServerMessage::TeamFormed {
            partner_name: inviter_name.clone(),
                          you_are: Seat::Second,
        });
    }

    let pool = pair_pool.entry(key.clone()).or_default();
    pool.push_back(pair_id);
    info!(
        "[lobby] пара {pair_id} сформирована ({inviter_name} + {from_name}), в пуле {} пар",
          pool.len()
    );
    try_start_pairs(&key, clients, pairs, pair_pool, next_game_id);
}

fn handle_accept(
    from: u64,
    inviter_name: &str,
    clients: &mut HashMap<u64, Client>,
    names: &HashMap<String, u64>,
    invites: &mut HashMap<u64, Invite>,
    pairs: &mut HashMap<u64, Pair>,
    pair_pool: &mut HashMap<Key, VecDeque<u64>>,
    next_pair_id: &mut u64,
    next_game_id: &mut u64,
) {
    let inviter_id = match names.get(&inviter_name.to_lowercase()).copied() {
        Some(x) => x,
        None => {
            if let Some(c) = clients.get_mut(&from) {
                let _ = c.tx.send(ServerMessage::InviteError {
                    code: "not_found".into(),
                                  message: format!("игрок {inviter_name} не найден"),
                });
            }
            return;
        }
    };

    // 1. Приглашение от inviter'а к from.
    let inv = invites.get(&inviter_id).cloned();
    let (first_id, second_id) = if let Some(ref inv) = inv {
        if inv.to == from {
            (inviter_id, from)
        } else {
            (0, 0)
        }
    } else {
        (0, 0)
    };

    // 2. Если не найдено — пробуем встречное.
    let (first_id, second_id) = if first_id == 0 {
        let inv2 = invites.get(&from).cloned();
        if let Some(ref inv) = inv2 {
            if inv.to == inviter_id {
                (from, inviter_id)
            } else {
                return;
            }
        } else {
            return;
        }
    } else {
        (first_id, second_id)
    };

    // 3. Удаляем оба приглашения.
    invites.remove(&inviter_id);
    invites.remove(&from);

    // 4. Создаём пару.
    create_pair_from_invite(
        first_id,
        second_id,
        clients,
        pairs,
        pair_pool,
        next_pair_id,
        next_game_id,
    );
}

fn handle_invite_timeout(clients: &mut HashMap<u64, Client>, from: u64, to: u64) {
    let from_name = clients
    .get(&from)
    .map(|c| c.name.clone())
    .unwrap_or_default();
    if let Some(c) = clients.get_mut(&from) {
        c.status = Status::Free;
        let _ = c.tx.send(ServerMessage::InviteError {
            code: "timeout".into(),
                          message: "приглашение истекло".into(),
        });
    }
    if let Some(t) = clients.get_mut(&to) {
        t.status = Status::Free;
        let _ = t.tx.send(ServerMessage::InviteCancelled { from_name });
    }
}











/// Кросс-сведение: одна пара в `Team2v2Pick` + двое одиночек в
/// `Team2v2Random` → запускаем игру (пара играет вместе, одиночки — вместе).
///
/// Вызывается из `ticker.tick()` (каждые 500 мс), поэтому
/// не блокирует и не зависит от порядка событий.
fn try_start_cross(
    pick_key: &Key,
    random_key: &Key,
    clients: &mut HashMap<u64, Client>,
    pairs: &mut HashMap<u64, Pair>,
    pair_pool: &mut HashMap<Key, VecDeque<u64>>,
    random_queues: &mut HashMap<Key, VecDeque<u64>>,
    next_game_id: &mut u64,
) -> bool {
    // В Pick-пуле должна быть ровно одна пара.
    let pair_id = {
        let pool = match pair_pool.get_mut(pick_key) {
            Some(p) => p,
            None => return false,
        };
        if pool.len() != 1 {
            return false;
        }
        match pool.front() {
            Some(&id) => id,
            None => return false,
        }
    };

    // В Random-очереди должно быть ≥2 одиночки.
    let (first_id, second_id) = {
        let q = match random_queues.get_mut(random_key) {
            Some(q) => q,
            None => return false,
        };
        if q.len() < 2 {
            return false;
        }
        (q.pop_front().unwrap(), q.pop_front().unwrap())
    };

    // Забираем пару из пула.
    pair_pool.get_mut(pick_key).unwrap().pop_front();
    let pair = match pairs.remove(&pair_id) {
        Some(p) => p,
        None => return false,
    };

    // Забираем клиентов.
    let mut white_first = match clients.remove(&pair.first) {
        Some(c) => c,
        None => return false,
    };
    let mut white_second = match clients.remove(&pair.second) {
        Some(c) => c,
        None => {
            clients.insert(pair.first, white_first);
            return false;
        }
    };
    let mut black_first = match clients.remove(&first_id) {
        Some(c) => c,
        None => {
            clients.insert(pair.first, white_first);
            clients.insert(pair.second, white_second);
            return false;
        }
    };
    let mut black_second = match clients.remove(&second_id) {
        Some(c) => c,
        None => {
            clients.insert(pair.first, white_first);
            clients.insert(pair.second, white_second);
            clients.insert(first_id, black_first);
            return false;
        }
    };

    *next_game_id += 1;
    let game_id = format!("g{}", next_game_id);
    let (initial, increment) = parse_time_control(&white_first.time_control);
    let state = GameState::new_team_2v2_with_clock(initial, increment);

    info!(
        "[lobby] cross team_2v2 {game_id}: {} + {} (pick-пара) vs {} + {} (random)",
          white_first.name, white_second.name, black_first.name, black_second.name
    );

    tokio::spawn(session::run_team_2v2(
        game_id, state,
        vec![
            (white_first.name.clone(), white_first.tx, white_first.rx.take().unwrap(), Seat::First, chess_logic::Color::White),
                                       (white_second.name.clone(), white_second.tx, white_second.rx.take().unwrap(), Seat::Second, chess_logic::Color::White),
                                       (black_first.name.clone(), black_first.tx, black_first.rx.take().unwrap(), Seat::First, chess_logic::Color::Black),
                                       (black_second.name.clone(), black_second.tx, black_second.rx.take().unwrap(), Seat::Second, chess_logic::Color::Black),
        ],
    ));
    true
}


fn try_match_explicit(
    id: u64,
    clients: &mut HashMap<u64, Client>,
    names: &HashMap<String, u64>,
    next_game_id: &mut u64,
) {
    // --- 1. Данные клиента id ---
    let (my_name, my_partner, my_opponents) = match clients.get(&id) {
        Some(c) => (
            c.name.to_lowercase(),
                    match &c.wants_partner {
                        Some(p) => p.clone(),
                    None => return,
                    },
                    match &c.wants_opponents {
                        Some(o) => o.clone(),
                    None => return,
                    },
        ),
        None => return,
    };

    // --- 2. Партнёр ---
    let partner_id = match names.get(&my_partner).copied() {
        Some(x) => x,
        None => return, // ещё не подключился
    };
    if partner_id == id {
        return;
    }

    let (partner_name, partner_mode, partner_partner, partner_opponents) =
    match clients.get(&partner_id) {
        Some(c) => (
            c.name.to_lowercase(),
                    c.mode,
                    c.wants_partner.clone(),
                    c.wants_opponents.clone(),
        ),
        None => return,
    };
    if partner_mode != Mode::Team2v2Explicit {
        return;
    }
    // Партнёр должен хотеть меня в партнёры.
    match partner_partner {
        Some(ref p) if *p == my_name => {}
        _ => return,
    }

    // --- 3. Соперники (согласованы у партнёров) ---
    let opp = match partner_opponents {
        Some(ref o) => o.clone(),
        None => return,
    };
    if !opponents_eq(&my_opponents, &opp) {
        return;
    }

    // --- 4. Оба соперника онлайн и в explicit ---
    let c1_id = match names.get(&my_opponents[0]).copied() {
        Some(x) => x,
        None => return,
    };
    let c2_id = match names.get(&my_opponents[1]).copied() {
        Some(x) => x,
        None => return,
    };
    if c1_id == id || c2_id == id || c1_id == partner_id || c2_id == partner_id {
        return;
    }

    let (c1_name, c1_mode, c1_partner, c1_opponents) = match clients.get(&c1_id) {
        Some(c) => (
            c.name.to_lowercase(),
                    c.mode,
                    c.wants_partner.clone(),
                    c.wants_opponents.clone(),
        ),
        None => return,
    };
    let (c2_name, c2_mode, c2_partner, c2_opponents) = match clients.get(&c2_id) {
        Some(c) => (
            c.name.to_lowercase(),
                    c.mode,
                    c.wants_partner.clone(),
                    c.wants_opponents.clone(),
        ),
        None => return,
    };
    if c1_mode != Mode::Team2v2Explicit || c2_mode != Mode::Team2v2Explicit {
        return;
    }

    // C1 хочет C2 в партнёры, C2 — C1.
    let c1_partner_ok = c1_partner.as_ref().map_or(false, |p| *p == c2_name);
    let c2_partner_ok = c2_partner.as_ref().map_or(false, |p| *p == c1_name);
    if !c1_partner_ok || !c2_partner_ok {
        return;
    }

    // C1 и C2 хотят нас в соперники.
    let us = vec![my_name.clone(), partner_name.clone()];
    let c1_opp_ok = c1_opponents.as_ref().map_or(false, |o| opponents_eq(&us, o));
    let c2_opp_ok = c2_opponents.as_ref().map_or(false, |o| opponents_eq(&us, o));
    if !c1_opp_ok || !c2_opp_ok {
        return;
    }

    // --- 5. Все согласованы. Определяем белых/чёрных. ---
    // Пара с меньшим min(client_id) — белые.
    let pair_a_min = id.min(partner_id);
    let pair_b_min = c1_id.min(c2_id);
    let (white_first_id, white_second_id, black_first_id, black_second_id) =
    if pair_a_min < pair_b_min {
        (id, partner_id, c1_id, c2_id)
    } else {
        (c1_id, c2_id, id, partner_id)
    };

    // --- 6. Забираем клиентов. ---
    let mut wf = match clients.remove(&white_first_id) {
        Some(c) => c,
        None => return,
    };
    let mut ws = match clients.remove(&white_second_id) {
        Some(c) => c,
        None => {
            clients.insert(white_first_id, wf);
            return;
        }
    };
    let mut bf = match clients.remove(&black_first_id) {
        Some(c) => c,
        None => {
            clients.insert(white_first_id, wf);
            clients.insert(white_second_id, ws);
            return;
        }
    };
    let mut bs = match clients.remove(&black_second_id) {
        Some(c) => c,
        None => {
            clients.insert(white_first_id, wf);
            clients.insert(white_second_id, ws);
            clients.insert(black_first_id, bf);
            return;
        }
    };

    *next_game_id += 1;
    let game_id = format!("g{}", next_game_id);
    let (initial, increment) = parse_time_control(&wf.time_control);
    let state = GameState::new_team_2v2_with_clock(initial, increment);

    info!(
        "[lobby] explicit team_2v2 {game_id}: {} + {} (белые) vs {} + {} (чёрные)",
          wf.name, ws.name, bf.name, bs.name
    );

    tokio::spawn(session::run_team_2v2(
        game_id,
        state,
        vec![
            (
                wf.name.clone(),
             wf.tx,
             wf.rx.take().unwrap(),
             Seat::First,
             chess_logic::Color::White,
            ),
            (
                ws.name.clone(),
             ws.tx,
             ws.rx.take().unwrap(),
             Seat::Second,
             chess_logic::Color::White,
            ),
            (
                bf.name.clone(),
             bf.tx,
             bf.rx.take().unwrap(),
             Seat::First,
             chess_logic::Color::Black,
            ),
            (
                bs.name.clone(),
             bs.tx,
             bs.rx.take().unwrap(),
             Seat::Second,
             chess_logic::Color::Black,
            ),
        ],
    ));
}

// ============================================================
// Вспомогательные функции (парсинг, удаление, explicit)
// ============================================================

fn parse_time_control(s: &str) -> (u64, u64) {
    let s = s.trim();
    if s.is_empty() || s == "none" {
        return (0, 0);
    }
    let mut parts = s.splitn(2, '+');
    let initial = parts
    .next()
    .and_then(|x| x.trim().parse::<u64>().ok())
    .unwrap_or(0);
    let increment = parts
    .next()
    .and_then(|x| x.trim().parse::<u64>().ok())
    .unwrap_or(0);
    (initial, increment)
}

fn parse_opponents(s: &str) -> Option<[String; 2]> {
    let parts: Vec<&str> = s
    .split(',')
    .map(|x| x.trim())
    .filter(|x| !x.is_empty())
    .collect();
    if parts.len() != 2 {
        return None;
    }
    let a = parts[0].to_lowercase();
    let b = parts[1].to_lowercase();
    if a == b {
        return None;
    }
    Some([a, b])
}

fn opponents_eq(a: &[String], b: &[String]) -> bool {
    if a.len() != 2 || b.len() != 2 {
        return false;
    }
    let a0 = a[0].to_lowercase();
    let a1 = a[1].to_lowercase();
    let b0 = b[0].to_lowercase();
    let b1 = b[1].to_lowercase();
    (a0 == b0 && a1 == b1) || (a0 == b1 && a1 == b0)
}

fn remove_client(
    id: u64,
    clients: &mut HashMap<u64, Client>,
    names: &mut HashMap<String, u64>,
    invites: &mut HashMap<u64, Invite>,
    queues: &mut HashMap<Key, VecDeque<u64>>,
    pairs: &mut HashMap<u64, Pair>,
    pair_pool: &mut HashMap<Key, VecDeque<u64>>,
) {
    let client = match clients.remove(&id) {
        Some(c) => c,
        None => return,
    };
    names.remove(&client.name.to_lowercase());

    remove_from_queues(id, queues, pair_pool, pairs);

    // Исходящее приглашение.
    let outgoing = invites.remove(&id);
    if let Some(inv) = outgoing {
        if let Some(target) = clients.get_mut(&inv.to) {
            target.status = Status::Free;
            let _ = target.tx.send(ServerMessage::InviteCancelled {
                from_name: client.name.clone(),
            });
        }
    }

    // Входящее приглашение.
    let inviter_id = invites
    .iter()
    .find(|(_, inv)| inv.to == id)
    .map(|(from, _)| *from);
    if let Some(inviter_id) = inviter_id {
        invites.remove(&inviter_id);
        if let Some(c) = clients.get_mut(&inviter_id) {
            c.status = Status::Free;
            let _ = c.tx.send(ServerMessage::InviteDeclined {
                by_name: client.name.clone(),
            });
        }
    }

    // Если был в паре — распустить.
    if let Status::InPair { pair_id } = client.status {
        if let Some(pair) = pairs.remove(&pair_id) {
            let partner_id = if pair.first == id { pair.second } else { pair.first };
            if let Some(p) = clients.get_mut(&partner_id) {
                p.status = Status::Free;
                let _ = p.tx.send(ServerMessage::PartnerLeft {
                    partner_name: client.name.clone(),
                });
            }
            if let Some(pool) = pair_pool.get_mut(&pair.key) {
                pool.retain(|pid| *pid != pair_id);
            }
        }
    }

    info!("[lobby] client {id} ({}) удалён", client.name);
}

fn remove_from_queues(
    id: u64,
    queues: &mut HashMap<Key, VecDeque<u64>>,
    pair_pool: &mut HashMap<Key, VecDeque<u64>>,
    pairs: &mut HashMap<u64, Pair>,
) {
    for q in queues.values_mut() {
        q.retain(|cid| *cid != id);
    }
    let pair_id = pairs
    .iter()
    .find_map(|(pid, p)| if p.first == id || p.second == id { Some(*pid) } else { None });
    if let Some(pid) = pair_id {
        if let Some(p) = pairs.remove(&pid) {
            if let Some(pool) = pair_pool.get_mut(&p.key) {
                pool.retain(|x| *x != pid);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parse_300_3() { assert_eq!(parse_time_control("300+3"), (300, 3)); }
    #[test]
    fn parse_none() { assert_eq!(parse_time_control("none"), (0, 0)); }
}


//! Игровая сессия: одна задача на пару (duel) или четвёрку (team_2v2).

use chess_logic::{
    Color, EndReason, GameAction, GameMode, GameState, MoveError, PieceType, Seat,
};
use chess_protocol::{ClientMessage, ServerMessage, Welcome};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::{interval, Instant};
use tracing::info;

// ============================================================
// SessionEvent
// ============================================================

#[derive(Debug)]
pub enum SessionEvent {
    FromClient(ClientMessage),
    Disconnected,
}

// ============================================================
// Duel
// ============================================================

pub async fn run_duel(
    game_id: String,
    mut state: GameState,
    white_name: String,
    black_name: String,
    white_tx: mpsc::UnboundedSender<ServerMessage>,
    black_tx: mpsc::UnboundedSender<ServerMessage>,
    mut white_rx: mpsc::UnboundedReceiver<SessionEvent>,
    mut black_rx: mpsc::UnboundedReceiver<SessionEvent>,
) {
    info!("[session {game_id}] duel: {white_name} (w) vs {black_name} (b)");

    let _ = white_tx.send(ServerMessage::Welcome(Welcome {
        color: Color::White,
        game_id: game_id.clone(),
                                                 opponent_name: Some(black_name.clone()),
                                                 seat: None,
                                                 partner_name: None,
                                                 opponents: None,
    }));
    let _ = black_tx.send(ServerMessage::Welcome(Welcome {
        color: Color::Black,
        game_id: game_id.clone(),
                                                 opponent_name: Some(white_name.clone()),
                                                 seat: None,
                                                 partner_name: None,
                                                 opponents: None,
    }));

    let _ = white_tx.send(ServerMessage::State(state.clone()));
    let _ = black_tx.send(ServerMessage::State(state.clone()));

    let mut tick = interval(Duration::from_secs(1));

    loop {
        if state.is_over() {
            let reason = state.end_reason.unwrap_or(EndReason::Checkmate);
            let hist_msg = ServerMessage::History { snapshots: state.history.clone() };
            let _ = white_tx.send(hist_msg.clone());
            let _ = black_tx.send(hist_msg);
            let _ = white_tx.send(ServerMessage::GameOver { reason });
            let _ = black_tx.send(ServerMessage::GameOver { reason });
            info!("[session {game_id}] duel окончен: {reason:?}");
            return;
        }
        tokio::select! {
            _ = tick.tick() => {
                if state.clock.is_none() { continue; }
                if state.tick() {
                    info!("[session {game_id}] duel: время истекло");
                }
                let _ = white_tx.send(ServerMessage::State(state.clone()));
                let _ = black_tx.send(ServerMessage::State(state.clone()));
            }

            Some(ev) = white_rx.recv() => {
                match ev {
                    SessionEvent::FromClient(msg) => {
                        handle_duel_msg(&mut state, Color::White, msg, &white_tx, &black_tx);
                    }
                    SessionEvent::Disconnected => {
                        let _ = black_tx.send(ServerMessage::OpponentDisconnect);
                        return;
                    }
                }
            }

            Some(ev) = black_rx.recv() => {
                match ev {
                    SessionEvent::FromClient(msg) => {
                        handle_duel_msg(&mut state, Color::Black, msg, &white_tx, &black_tx);
                    }
                    SessionEvent::Disconnected => {
                        let _ = white_tx.send(ServerMessage::OpponentDisconnect);
                        return;
                    }
                }
            }

            else => return,
        }
    }
}

fn handle_duel_msg(
    state: &mut GameState,
    color: Color,
    msg: ClientMessage,
    white_tx: &mpsc::UnboundedSender<ServerMessage>,
    black_tx: &mpsc::UnboundedSender<ServerMessage>,
) {
    let tx = if color == Color::White { white_tx } else { black_tx };
    let other_tx = if color == Color::White { black_tx } else { white_tx };

    match msg {
        ClientMessage::MakeMove { from, to, promotion } => {
            let res = state.apply_move(from, to, promotion);
            if !res.ok {
                let _ = tx.send(ServerMessage::Error {
                    message: "ход отклонён".into(),
                                code: None,
                });
                return;
            }
            broadcast(state, white_tx, black_tx);
        }
        ClientMessage::Castle { a, c } => {
            let res = state.apply_castle(a, c);
            if !res.ok {
                let _ = tx.send(ServerMessage::Error {
                    message: "рокировка отклонена".into(),
                                code: None,
                });
                return;
            }
            broadcast(state, white_tx, black_tx);
        }
        ClientMessage::Drop { piece, to } => {
            let res = state.apply_drop(color, piece, to);
            if !res.ok {
                let _ = tx.send(ServerMessage::Error {
                    message: "постановка отклонена".into(),
                                code: None,
                });
                return;
            }
            broadcast(state, white_tx, black_tx);
        }
        ClientMessage::Resign => {
            let _ = state.resign(color);
            broadcast(state, white_tx, black_tx);
        }
        ClientMessage::OfferDraw => {
            let _ = other_tx.send(ServerMessage::DrawOffer { by: color, by_names: None });
        }
        ClientMessage::AcceptDraw => {
            state.agree_draw();
            broadcast(state, white_tx, black_tx);
        }
        ClientMessage::DeclineDraw => {
            let _ = other_tx.send(ServerMessage::DrawDeclined);
        }
        ClientMessage::RequestState => {
            let _ = tx.send(ServerMessage::State(state.clone()));
        }
        ClientMessage::Hello(_) => {}
        ClientMessage::ReturnRoyal { .. }
        | ClientMessage::InvitePartner { .. }
        | ClientMessage::CancelInvite
        | ClientMessage::AcceptInvite { .. }
        | ClientMessage::DeclineInvite { .. }
        | ClientMessage::LeaveLobby
        | ClientMessage::OfferDrawRequest
        | ClientMessage::OfferDrawConfirm
        | ClientMessage::OfferDrawDecline
        | ClientMessage::OfferResignRequest
        | ClientMessage::OfferResignConfirm
        | ClientMessage::OfferResignDecline => {
            let _ = tx.send(ServerMessage::Error {
                message: "команда не поддерживается в duel".into(),
                            code: Some("not_supported".into()),
            });
        }
    }
}

fn broadcast(
    state: &GameState,
    white_tx: &mpsc::UnboundedSender<ServerMessage>,
    black_tx: &mpsc::UnboundedSender<ServerMessage>,
) {
    let msg = ServerMessage::State(state.clone());
    let _ = white_tx.send(msg.clone());
    let _ = black_tx.send(msg);
}

// ============================================================
// Team 2v2
// ============================================================

const DRAW_PHASE_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_ILLEGAL_STREAK: u32 = 5;

pub struct PlayerSlot {
    pub name: String,
    pub tx: mpsc::UnboundedSender<ServerMessage>,
    pub rx: mpsc::UnboundedReceiver<SessionEvent>,
    pub seat: Seat,
    pub color: Color,
}

#[derive(Clone, Debug)]
enum DrawPhase {
    Idle,
    AwaitingPartner { initiator_seat: Seat, color: Color, deadline: Instant },
    AwaitingOpponents { from_color: Color, from_names: [String; 2], accepted: [bool; 2], deadline: Instant },
    AwaitingResignPartner { initiator_seat: Seat, color: Color, deadline: Instant },
}

pub async fn run_team_2v2(
    game_id: String,
    mut state: GameState,
    players: Vec<(
        String,
        mpsc::UnboundedSender<ServerMessage>,
        mpsc::UnboundedReceiver<SessionEvent>,
        Seat,
        Color,
    )>,
) {
    if state.mode != GameMode::Team2v2 {
        state.mode = GameMode::Team2v2;
    }
    if players.len() != 4 {
        info!("[session {game_id}] team_2v2: ожидалось 4 игрока, пришло {}", players.len());
        return;
    }

    let mut slots: Vec<Option<PlayerSlot>> = (0..4).map(|_| None).collect();
    for (name, tx, rx, seat, color) in players {
        let idx = slot_index(color, seat);
        slots[idx] = Some(PlayerSlot { name, tx, rx, seat, color });
    }
    let mut slots: Vec<PlayerSlot> = slots
    .into_iter()
    .map(|s| s.expect("все 4 слота должны быть заполнены"))
    .collect();

    let white_names = [slots[0].name.clone(), slots[1].name.clone()];
    let black_names = [slots[2].name.clone(), slots[3].name.clone()];

    for slot in slots.iter_mut() {
        let (partner_name, opponents) = match slot.color {
            Color::White => (white_names[1 - slot.seat.index()].clone(), black_names.clone()),
            Color::Black => (black_names[1 - slot.seat.index()].clone(), white_names.clone()),
        };
        let _ = slot.tx.send(ServerMessage::Welcome(Welcome {
            color: slot.color,
            game_id: game_id.clone(),
                                                    opponent_name: None,
                                                    seat: Some(slot.seat),
                                                    partner_name: Some(partner_name),
                                                    opponents: Some(opponents),
        }));
        let _ = slot.tx.send(ServerMessage::State(state.clone()));
    }

    info!(
        "[session {game_id}] team_2v2: {} + {} (w) vs {} + {} (b)",
          white_names[0], white_names[1], black_names[0], black_names[1]
    );

    let mut rx0 = extract_rx(&mut slots[0]);
    let mut rx1 = extract_rx(&mut slots[1]);
    let mut rx2 = extract_rx(&mut slots[2]);
    let mut rx3 = extract_rx(&mut slots[3]);

    let mut alive0 = true;
    let mut alive1 = true;
    let mut alive2 = true;
    let mut alive3 = true;

    let mut error_counts = [0u32; 4];
    let mut draw_phase = DrawPhase::Idle;
    let mut tick = interval(Duration::from_secs(1));

    loop {
        if state.is_over() {
            let reason = state.end_reason.unwrap_or(EndReason::Checkmate);
            broadcast_all(&state, &slots);
            let hist_msg = ServerMessage::History { snapshots: state.history.clone() };
            for slot in slots.iter() {
                let _ = slot.tx.send(hist_msg.clone());
            }
            for slot in slots.iter() {
                let _ = slot.tx.send(ServerMessage::GameOver { reason });
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
            info!("[session {game_id}] team_2v2 окончен: {reason:?}");
            return;
        }

        draw_phase = check_draw_timeout(&game_id, &mut slots, draw_phase);

        tokio::select! {
            _ = tick.tick() => {
                if state.clock.is_none() { continue; }
                if state.tick() {
                    info!("[session {game_id}] team_2v2: время истекло");
                }
                broadcast_all(&state, &slots);
            }

            res = rx0.recv(), if alive0 => {
                match res {
                    Some(ev) => process_slot_event(&game_id, 0, ev, &mut state, &mut slots, &mut draw_phase, &mut error_counts),
                    None => {
                        alive0 = false;
                        info!("[session {game_id}] slot 0 rx закрыт");
                        process_slot_event(&game_id, 0, SessionEvent::Disconnected, &mut state, &mut slots, &mut draw_phase, &mut error_counts);
                    }
                }
            }
            res = rx1.recv(), if alive1 => {
                match res {
                    Some(ev) => process_slot_event(&game_id, 1, ev, &mut state, &mut slots, &mut draw_phase, &mut error_counts),
                    None => {
                        alive1 = false;
                        info!("[session {game_id}] slot 1 rx закрыт");
                        process_slot_event(&game_id, 1, SessionEvent::Disconnected, &mut state, &mut slots, &mut draw_phase, &mut error_counts);
                    }
                }
            }
            res = rx2.recv(), if alive2 => {
                match res {
                    Some(ev) => process_slot_event(&game_id, 2, ev, &mut state, &mut slots, &mut draw_phase, &mut error_counts),
                    None => {
                        alive2 = false;
                        info!("[session {game_id}] slot 2 rx закрыт");
                        process_slot_event(&game_id, 2, SessionEvent::Disconnected, &mut state, &mut slots, &mut draw_phase, &mut error_counts);
                    }
                }
            }
            res = rx3.recv(), if alive3 => {
                match res {
                    Some(ev) => process_slot_event(&game_id, 3, ev, &mut state, &mut slots, &mut draw_phase, &mut error_counts),
                    None => {
                        alive3 = false;
                        info!("[session {game_id}] slot 3 rx закрыт");
                        process_slot_event(&game_id, 3, SessionEvent::Disconnected, &mut state, &mut slots, &mut draw_phase, &mut error_counts);
                    }
                }
            }

            else => {
                info!("[session {game_id}] team_2v2: все каналы закрыты — завершаюсь");
                return;
            }
        }
    }
}

fn extract_rx(slot: &mut PlayerSlot) -> mpsc::UnboundedReceiver<SessionEvent> {
    let (tx, rx) = mpsc::unbounded_channel::<SessionEvent>();
    drop(tx);
    std::mem::replace(&mut slot.rx, rx)
}

fn process_slot_event(
    game_id: &str,
    idx: usize,
    ev: SessionEvent,
    state: &mut GameState,
    slots: &mut [PlayerSlot],
    draw_phase: &mut DrawPhase,
    error_counts: &mut [u32; 4],
) {
    match ev {
        SessionEvent::Disconnected => {
            let name = slots[idx].name.clone();
            info!("[session {game_id}] team_2v2: {name} отключился");
            let color = slots[idx].color;
            let seat = slots[idx].seat;

            error_counts[idx] = 0;
            state.board.remove_all_of_player(color, seat);
            let team = match color {
                Color::White => &mut state.white_team,
                Color::Black => &mut state.black_team,
            };
            let p = team.player_mut(seat);
            p.alive = false;
            p.prince_in_holding = false;
            p.princess_in_holding = false;

            check_team_eliminated(state, game_id);
            broadcast_all(state, slots);
        }
        SessionEvent::FromClient(msg) => {
            handle_team_msg(game_id, idx, msg, state, slots, draw_phase, error_counts);
        }
    }
}

fn check_team_eliminated(state: &mut GameState, game_id: &str) {
    if state.is_over() { return; }
    let white_alive = state.white_team.players.iter().any(|p| p.alive);
    let black_alive = state.black_team.players.iter().any(|p| p.alive);
    if !white_alive && !black_alive {
        state.is_draw = true;
        state.end_reason = Some(EndReason::Draw);
        info!("[session {game_id}] обе команды выбыли — ничья");
    } else if !white_alive {
        state.winner = Some(Color::Black);
        state.end_reason = Some(EndReason::Resign);
        info!("[session {game_id}] белые выбыли — победа чёрных");
    } else if !black_alive {
        state.winner = Some(Color::White);
        state.end_reason = Some(EndReason::Resign);
        info!("[session {game_id}] чёрные выбыли — победа белых");
    }
}

fn handle_team_msg(
    game_id: &str,
    idx: usize,
    msg: ClientMessage,
    state: &mut GameState,
    slots: &mut [PlayerSlot],
    draw_phase: &mut DrawPhase,
    error_counts: &mut [u32; 4],
) {
    let (color, seat, name) = {
        let s = &slots[idx];
        (s.color, s.seat, s.name.clone())
    };

    match msg {
        ClientMessage::MakeMove { from, to, promotion } => {
            let res = state.apply_action(color, seat, GameAction::Move { from, to, promotion });
            if !res.ok {
                error_counts[idx] += 1;
                if error_counts[idx] >= MAX_ILLEGAL_STREAK {
                    info!("[session {game_id}] {name} (slot {idx}): {MAX_ILLEGAL_STREAK} ошибок подряд — принудительный Disconnected");
                    process_slot_event(game_id, idx, SessionEvent::Disconnected, state, slots, draw_phase, error_counts);
                    return;
                }
                if let Some(MoveError::RoyalInHolding(pt)) = res.error {
                    let piece_name = match pt {
                        PieceType::Prince => "Принц",
                        PieceType::Princess => "Принцесса",
                        _ => "Royal",
                    };
                    send_error(slots, idx, &format!("{} в запасе, сначала верните его на доску", piece_name), None);
                } else {
                    send_error(slots, idx, "ход отклонён", res.error.map(|e| format!("{e:?}")));
                }
                return;
            }
            error_counts[idx] = 0;
            broadcast_all(state, slots);
        }
        ClientMessage::Castle { a, c } => {
            let res = state.apply_action(color, seat, GameAction::Castle { a, c });
            if !res.ok {
                error_counts[idx] += 1;
                if error_counts[idx] >= MAX_ILLEGAL_STREAK {
                    info!("[session {game_id}] {name} (slot {idx}): {MAX_ILLEGAL_STREAK} ошибок подряд — Disconnected");
                    process_slot_event(game_id, idx, SessionEvent::Disconnected, state, slots, draw_phase, error_counts);
                    return;
                }
                send_error(slots, idx, "рокировка отклонена", res.error.map(|e| format!("{e:?}")));
                return;
            }
            error_counts[idx] = 0;
            broadcast_all(state, slots);
        }
        ClientMessage::Drop { piece, to } => {
            let res = state.apply_action(color, seat, GameAction::Drop { piece, to });
            if !res.ok {
                error_counts[idx] += 1;
                if error_counts[idx] >= MAX_ILLEGAL_STREAK {
                    info!("[session {game_id}] {name} (slot {idx}): {MAX_ILLEGAL_STREAK} ошибок подряд — Disconnected");
                    process_slot_event(game_id, idx, SessionEvent::Disconnected, state, slots, draw_phase, error_counts);
                    return;
                }
                send_error(slots, idx, "постановка отклонена", res.error.map(|e| format!("{e:?}")));
                return;
            }
            error_counts[idx] = 0;
            broadcast_all(state, slots);
        }
        ClientMessage::ReturnRoyal { royal_pt, to } => {
            let res = state.apply_action(color, seat, GameAction::ReturnRoyal { royal_pt, to });
            if !res.ok {
                error_counts[idx] += 1;
                if error_counts[idx] >= MAX_ILLEGAL_STREAK {
                    info!("[session {game_id}] {name} (slot {idx}): {MAX_ILLEGAL_STREAK} ошибок подряд — Disconnected");
                    process_slot_event(game_id, idx, SessionEvent::Disconnected, state, slots, draw_phase, error_counts);
                    return;
                }
                send_error(slots, idx, "возврат royal отклонён", res.error.map(|e| format!("{e:?}")));
                return;
            }
            error_counts[idx] = 0;
            broadcast_all(state, slots);
        }

        ClientMessage::OfferDrawRequest => {
            if !matches!(draw_phase, DrawPhase::Idle) {
                send_error(slots, idx, "уже есть активное предложение", Some("draw_in_progress".into()));
                return;
            }
            let partner_idx = partner_index(color, seat);
            let deadline = Instant::now() + DRAW_PHASE_TIMEOUT;
            *draw_phase = DrawPhase::AwaitingPartner { initiator_seat: seat, color, deadline };
            let _ = slots[partner_idx].tx.send(ServerMessage::DrawRequestToPartner {
                from_name: name.clone(),
            });
            info!("[session {game_id}] {name} просит партнёра подтвердить ничью");
        }
        ClientMessage::OfferDrawConfirm => {
            let (initiator_seat, init_color) = match draw_phase {
                DrawPhase::AwaitingPartner { initiator_seat, color, .. } => (*initiator_seat, *color),
                _ => { send_error(slots, idx, "нет активного предложения", None); return; }
            };
            if init_color != color { send_error(slots, idx, "чужое предложение", None); return; }
            if seat == initiator_seat { send_error(slots, idx, "нельзя подтвердить своё", None); return; }
            let from_names = match color {
                Color::White => [slots[0].name.clone(), slots[1].name.clone()],
                Color::Black => [slots[2].name.clone(), slots[3].name.clone()],
            };
            let deadline = Instant::now() + DRAW_PHASE_TIMEOUT;
            *draw_phase = DrawPhase::AwaitingOpponents {
                from_color: init_color, from_names: from_names.clone(), accepted: [false, false], deadline,
            };
            for opp_idx in opponent_indices(init_color) {
                let _ = slots[opp_idx].tx.send(ServerMessage::DrawOffer {
                    by: init_color,
                    by_names: Some(from_names.clone()),
                });
            }
        }
        ClientMessage::OfferDrawDecline => {
            if let DrawPhase::AwaitingPartner { initiator_seat, color: c, .. } = draw_phase {
                if *c == color && seat != *initiator_seat {
                    let init_idx = slot_index(*c, *initiator_seat);
                    let _ = slots[init_idx].tx.send(ServerMessage::DrawRequestDeclined);
                }
            }
            *draw_phase = DrawPhase::Idle;
        }
        ClientMessage::AcceptDraw => {
            if let DrawPhase::AwaitingOpponents { from_color, ref mut accepted, .. } = draw_phase {
                if *from_color == color { send_error(slots, idx, "нельзя согласиться со своей", None); return; }
                let opp_seat_idx = seat.index();
                if accepted[opp_seat_idx] { send_error(slots, idx, "уже согласны", None); return; }
                accepted[opp_seat_idx] = true;
                for i in 0..4 {
                    let _ = slots[i].tx.send(ServerMessage::DrawOfferAccepted { by_color: color, by_seat: seat });
                }
                if accepted[0] && accepted[1] {
                    state.agree_draw();
                    *draw_phase = DrawPhase::Idle;
                    broadcast_all(state, slots);
                    return;
                }
            } else {
                send_error(slots, idx, "нет активного предложения", None);
            }
        }
        ClientMessage::DeclineDraw => {
            if let DrawPhase::AwaitingOpponents { from_color, .. } = draw_phase {
                if *from_color == color { send_error(slots, idx, "нельзя отклонить свою", None); return; }
                *draw_phase = DrawPhase::Idle;
                for i in 0..4 {
                    let _ = slots[i].tx.send(ServerMessage::DrawDeclined);
                }
                info!("[session {game_id}] ничья отклонена");
            } else {
                send_error(slots, idx, "нет активного предложения", None);
            }
        }
        ClientMessage::Resign => {
            send_error(slots, idx, "сдача в team_2v2 запрещена", Some("resign_not_allowed".into()));
        }
        ClientMessage::RequestState => {
            let _ = slots[idx].tx.send(ServerMessage::State(state.clone()));
        }
        ClientMessage::Hello(_) => {}
        ClientMessage::OfferResignRequest => {
            if !matches!(draw_phase, DrawPhase::Idle) {
                send_error(slots, idx, "уже есть активное предложение", None);
                return;
            }
            let partner_idx = partner_index(color, seat);
            let deadline = Instant::now() + DRAW_PHASE_TIMEOUT;
            *draw_phase = DrawPhase::AwaitingResignPartner { initiator_seat: seat, color, deadline };
            let _ = slots[partner_idx].tx.send(ServerMessage::ResignRequestToPartner { from_name: name.clone() });
        }
        ClientMessage::OfferResignConfirm => {
            if let DrawPhase::AwaitingResignPartner { initiator_seat, color: c, .. } = draw_phase {
                if *c == color && seat != *initiator_seat {
                    state.resign(color);
                    *draw_phase = DrawPhase::Idle;
                    broadcast_all(state, slots);
                    return;
                }
            }
            send_error(slots, idx, "нет активного запроса на сдачу", None);
        }
        ClientMessage::OfferResignDecline => {
            if let DrawPhase::AwaitingResignPartner { initiator_seat, color: c, .. } = draw_phase {
                if *c == color && seat != *initiator_seat {
                    let init_idx = slot_index(*c, *initiator_seat);
                    let _ = slots[init_idx].tx.send(ServerMessage::ResignRequestDeclined);
                }
            }
            *draw_phase = DrawPhase::Idle;
        }

        ClientMessage::InvitePartner { .. }
        | ClientMessage::CancelInvite
        | ClientMessage::AcceptInvite { .. }
        | ClientMessage::DeclineInvite { .. }
        | ClientMessage::LeaveLobby
        | ClientMessage::OfferDraw => {
            let _ = slots[idx].tx.send(ServerMessage::Error {
                message: "команда не поддерживается в team_2v2".into(),
                                       code: Some("not_supported".into()),
            });
        }
    }
}

fn check_draw_timeout(game_id: &str, slots: &mut [PlayerSlot], phase: DrawPhase) -> DrawPhase {
    let now = Instant::now();
    match &phase {
        DrawPhase::AwaitingPartner { deadline, initiator_seat, color } => {
            if now >= *deadline {
                let init_idx = slot_index(*color, *initiator_seat);
                let _ = slots[init_idx].tx.send(ServerMessage::DrawRequestDeclined);
                info!("[session {game_id}] таймаут подтверждения ничьей партнёром");
                return DrawPhase::Idle;
            }
        }
        DrawPhase::AwaitingOpponents { deadline, .. } => {
            if now >= *deadline {
                for i in 0..4 {
                    let _ = slots[i].tx.send(ServerMessage::DrawDeclined);
                }
                info!("[session {game_id}] таймаут ответа соперников на ничью");
                return DrawPhase::Idle;
            }
        }
        DrawPhase::AwaitingResignPartner { deadline, initiator_seat, color } => {
            if now >= *deadline {
                let init_idx = slot_index(*color, *initiator_seat);
                let _ = slots[init_idx].tx.send(ServerMessage::ResignRequestDeclined);
                info!("[session {game_id}] таймаут подтверждения сдачи партнёром");
                return DrawPhase::Idle;
            }
        }
        DrawPhase::Idle => {}
    }
    phase
}

fn broadcast_all(state: &GameState, slots: &[PlayerSlot]) {
    let msg = ServerMessage::State(state.clone());
    for (i, s) in slots.iter().enumerate() {
        if s.tx.send(msg.clone()).is_err() {
            // Слот мёртв — writer клиента завершился, State не доставлен.
            // run_team_2v2 продолжает работать (reader ещё жив),
            // но этот игрок больше не получит обновлений.
            tracing::warn!(
                "[session] slot {i} ({}) мёртв — State не доставлен",
                           s.name
            );
        }
    }
}

fn send_error(slots: &[PlayerSlot], idx: usize, message: &str, code: Option<String>) {
    if slots[idx]
        .tx
        .send(ServerMessage::Error {
            message: message.to_string(),
              code,
        })
        .is_err()
        {
            tracing::warn!(
                "[session] slot {idx} ({}) мёртв — Error не доставлен",
                           slots[idx].name
            );
        }
}

fn partner_index(color: Color, seat: Seat) -> usize {
    slot_index(color, seat.other())
}

fn slot_index(color: Color, seat: Seat) -> usize {
    match (color, seat) {
        (Color::White, Seat::First) => 0,
        (Color::White, Seat::Second) => 1,
        (Color::Black, Seat::First) => 2,
        (Color::Black, Seat::Second) => 3,
    }
}

fn opponent_indices(color: Color) -> [usize; 2] {
    match color {
        Color::White => [2, 3],
        Color::Black => [0, 1],
    }
}

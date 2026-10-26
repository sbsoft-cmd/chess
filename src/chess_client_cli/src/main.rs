//! CLI-клиент: подключается к серверу через chess_client_net.
//!
//! Режимы:
//!   - duel:         `mode duel` (по умолчанию), ходы из stdin;
//!   - team_random:  `mode team_random` → ждём 4 одиночки;
//!   - team_pick:    `mode team_pick` + `partner <name>` → `invite <name>`.

use anyhow::Result;
use chess_client_net::{spawn_client, ClientCommand, ClientEvent};
use chess_logic::{PieceType, Seat, Square};
use chess_protocol::Hello;
use std::sync::mpsc as std_mpsc;
use std::time::Duration;

/// Текущий режим CLI.
#[derive(Clone, Debug, PartialEq)]
enum CliMode {
    Duel,
    TeamRandom,
    TeamPick,
}

impl Default for CliMode {
    fn default() -> Self { CliMode::Duel }
}

impl CliMode {
    fn as_hello_str(&self) -> &'static str {
        match self {
            CliMode::Duel => "duel",
            CliMode::TeamRandom => "team_2v2_random",
            CliMode::TeamPick => "team_2v2_pick",
        }
    }
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
    .with_env_filter("info")
    .init();

    let mut args = std::env::args().skip(1);
    let name = args.next().unwrap_or_else(|| "cli".into());
    let host = args.next().unwrap_or_else(|| "127.0.0.1".into());

    println!("[cli {name}] подключение к {host}:8080");

    let mut mode = CliMode::default();
    let mut partner_name = String::new();
    let mut net: Option<(
        tokio::sync::mpsc::UnboundedSender<ClientCommand>,
        std::sync::Arc<std::sync::Mutex<std_mpsc::Receiver<ClientEvent>>>,
    )> = None;

    let stdin = std::io::stdin();
    let mut line = String::new();

    print_help();

    loop {
        line.clear();
        let n = stdin.read_line(&mut line)?;
        if n == 0 { break; }
        let trimmed = line.trim();
        if trimmed.is_empty() { continue; }

        let parts: Vec<&str> = trimmed.split_whitespace().collect();

        // === Управление соединением до старта ===
        if net.is_none() {
            match parts.as_slice() {
                ["mode", m] => {
                    mode = match *m {
                        "duel" => CliMode::Duel,
                        "team_random" => CliMode::TeamRandom,
                        "team_pick" => CliMode::TeamPick,
                        other => {
                            eprintln!("неизвестный режим: {other}");
                            continue;
                        }
                    };
                    println!("mode = {:?}", mode);
                    continue;
                }
                ["partner", p] => {
                    partner_name = p.to_string();
                    println!("partner = {p}");
                    continue;
                }
                ["connect"] | ["start"] => {
                    // Подключаемся.
                    let hello = Hello {
                        version: Some("0.1".into()),
                        name: name.clone(),
                        variant: "10x10".into(),
                        time_control: "300+3".into(),
                        mode: mode.as_hello_str().to_string(),
                        partner_name: if partner_name.trim().is_empty() {
                            None
                        } else {
                            Some(partner_name.trim().to_string())
                        },
                    };

                    let (cmd_tx, ev_rx, _handle) = spawn_client(host.clone(), 8080, hello);
                    let ev_rx = std::sync::Arc::new(std::sync::Mutex::new(ev_rx));
                    {
                        let ev_rx = ev_rx.clone();
                        std::thread::spawn(move || loop {
                            let ev = {
                                let rx = ev_rx.lock().unwrap();
                                rx.recv_timeout(Duration::from_millis(100))
                            };
                            match ev {
                                Ok(event) => print_event(&event),
                                           Err(std_mpsc::RecvTimeoutError::Timeout) => continue,
                                           Err(std_mpsc::RecvTimeoutError::Disconnected) => break,
                            }
                        });
                    }
                    net = Some((cmd_tx, ev_rx));
                    println!("подключено (mode={:?})", mode);
                    continue;
                }
                _ => {
                    eprintln!("сначала настройте режим и нажмите `connect` (или `start`).");
                    print_help();
                    continue;
                }
            }
        }

        // === Соединение есть — команды ===
        let cmd_tx = net.as_ref().unwrap().0.clone();

        // Сохраняем mode на случай переключения в рантайме.
        let current_mode = mode.clone();

        let cmd = match parts.as_slice() {
            ["quit"] => break,

            // --- Duel / общие ---
            ["resign"] => ClientCommand::Resign,
            ["draw"] => {
                if current_mode == CliMode::Duel {
                    ClientCommand::OfferDraw
                } else {
                    ClientCommand::OfferDrawRequest
                }
            }
            ["accept_draw"] => ClientCommand::AcceptDraw,
            ["decline_draw"] => ClientCommand::DeclineDraw,

            // --- Duel: обычные ходы ---
            ["castle", ac, ar, cc, cr] => {
                let a = Square::new(ac.parse()?, ar.parse()?);
                let c = Square::new(cc.parse()?, cr.parse()?);
                ClientCommand::Castle { a, c }
            }
            ["drop", pt, c, r] => {
                let piece = parse_piece(pt)?;
                let to = Square::new(c.parse()?, r.parse()?);
                ClientCommand::Drop { piece, to }
            }
            [fc, fr, tc, tr] => {
                let from = Square::new(fc.parse()?, fr.parse()?);
                let to = Square::new(tc.parse()?, tr.parse()?);
                ClientCommand::MakeMove { from, to, promotion: None }
            }

            // --- 2×2: возврат royal'а ---
            ["return", pt, c, r] => {
                let royal_pt = parse_piece(pt)?;
                if !matches!(royal_pt, PieceType::Prince | PieceType::Princess) {
                    eprintln!("return: ожидается P2 или P3");
                    continue;
                }
                let to = Square::new(c.parse()?, r.parse()?);
                ClientCommand::ReturnRoyal { royal_pt, to }
            }

            // --- 2×2: лобби / приглашения ---
            ["invite", target] => ClientCommand::InvitePartner {
                target_name: target.to_string(),
            },
            ["cancel_invite"] => ClientCommand::CancelInvite,
            ["accept", from] => ClientCommand::AcceptInvite {
                from_name: from.to_string(),
            },
            ["decline", from] => ClientCommand::DeclineInvite {
                from_name: from.to_string(),
            },
            ["leave"] => ClientCommand::LeaveLobby,

            // --- 2×2: двухфазная ничья ---
            ["draw_req"] => ClientCommand::OfferDrawRequest,
            ["draw_confirm"] => ClientCommand::OfferDrawConfirm,
            ["draw_decline"] => ClientCommand::OfferDrawDecline,

            _ => {
                eprintln!("неизвестная команда: {trimmed}");
                continue;
            }
        };

        if cmd_tx.send(cmd).is_err() {
            eprintln!("[cli] соединение закрыто");
            break;
        }
    }

    let _ = net.map(|(tx, _)| tx.send(ClientCommand::Shutdown));
    Ok(())
}

fn parse_piece(s: &str) -> Result<PieceType> {
    Ok(match s {
        "P" => PieceType::Pawn,
       "R" => PieceType::Rook,
       "N" => PieceType::Knight,
       "B" => PieceType::Bishop,
       "Q" => PieceType::Queen,
       "K" => PieceType::King,
       "N2" => PieceType::Eagle,
       "R2" => PieceType::Cheetah,
       "J" => PieceType::Genie,
       "P2" => PieceType::Prince,
       "P3" => PieceType::Princess,
       other => anyhow::bail!("неизвестный тип фигуры: {other}"),
    })
}

fn print_help() {
    println!(
        "Команды до connect:\n\
\x20 mode <duel|team_random|team_pick>\n\
\x20 partner <name>      — имя партнёра (для team_pick)\n\
\x20 connect | start    — подключиться\n\
\n\
Команды после connect:\n\
\x20 <fc> <fr> <tc> <tr>  — ход (например `4 1 4 3` = a2→a4)\n\
\x20 castle <ac> <ar> <cc> <cr>\n\
\x20 drop <P|R|N|B|Q|K|N2|R2|J> <col> <row>\n\
\x20 return <P2|P3> <col> <row>   — вернуть royal из укрытия\n\
\x20 invite <name> | accept <name> | decline <name> | cancel_invite | leave\n\
\x20 draw_req | draw_confirm | draw_decline\n\
\x20 draw | accept_draw | decline_draw | resign | quit"
    );
}

fn print_event(ev: &ClientEvent) {
    match ev {
        ClientEvent::Connected => println!("CONNECTED"),
        ClientEvent::Welcome {
            color,
            game_id,
            opponent_name,
            seat,
            partner_name,
            opponents,
        } => {
            println!(
                "WELCOME color={:?} game_id={} opponent={:?} seat={:?} partner={:?} opponents={:?}",
                color, game_id, opponent_name, seat, partner_name, opponents
            );
        }
        ClientEvent::State(state) => {
            let clock_str = match &state.clock {
                Some(c) => format!(" w={}s b={}s", c.white.as_secs(), c.black.as_secs()),
                None => String::new(),
            };
            println!(
                "STATE current={:?} winner={:?} over={} pieces={}{}",
                state.current_player,
                state.winner,
                state.is_over(),
                     state.board.count(),
                     clock_str,
            );
        }
        ClientEvent::GameOver { reason } => println!("GAME OVER: {reason:?}"),
        ClientEvent::ServerError { message, code } => {
            println!("ERROR: {message} (code={code:?})")
        }
        ClientEvent::DrawOffer { by, by_names } => {
            println!("DRAW OFFER from {by:?} names={by_names:?}")
        }
        ClientEvent::DrawDeclined => println!("DRAW DECLINED"),
        ClientEvent::OpponentDisconnected => println!("OPPONENT DISCONNECTED"),
        ClientEvent::ConnectionLost { reason } => println!("CONNECTION LOST: {reason}"),
        ClientEvent::InviteSent { to_name } => println!("INVITE SENT to {to_name}"),
        ClientEvent::InviteReceived { from_name, variant, time_control } => {
            println!("INVITE RECEIVED from {from_name} ({variant} {time_control})")
        }
        ClientEvent::InviteCancelled { from_name } => println!("INVITE CANCELLED {from_name}"),
        ClientEvent::InviteDeclined { by_name } => println!("INVITE DECLINED by {by_name}"),
        ClientEvent::InviteError { code, message } => {
            println!("INVITE ERROR {code}: {message}")
        }
        ClientEvent::TeamFormed { partner_name, you_are } => {
            println!("TEAM FORMED partner={partner_name} you_are={you_are:?}")
        }
        ClientEvent::PartnerLeft { partner_name } => println!("PARTNER LEFT {partner_name}"),
        ClientEvent::DrawRequestToPartner { from_name } => {
            println!("DRAW REQUEST from partner {from_name} (ответьте draw_confirm / draw_decline)")
        }
        ClientEvent::DrawRequestDeclined => println!("DRAW REQUEST DECLINED"),
        ClientEvent::DrawOfferAccepted { by_color, by_seat } => {
            println!("DRAW OFFER ACCEPTED by {:?} {:?} (ждём второго)", by_color, by_seat)
        }


        ClientEvent::History { snapshots } => {
            println!("HISTORY: {} snapshots", snapshots.len())
        }
        ClientEvent::ResignRequestToPartner { from_name } => {
            println!("RESIGN REQUEST from partner {from_name}")
        }
        ClientEvent::ResignRequestDeclined => println!("RESIGN REQUEST DECLINED"),
        ClientEvent::TeamResigned { by_color } => {
            println!("TEAM RESIGNED by {:?}", by_color)
        }
    }


}

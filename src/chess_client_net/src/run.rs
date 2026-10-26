//! Внутренняя логика сетевого клиента.
//!
//! `spawn_client` создаёт Tokio runtime в отдельном потоке.
//! Reader-задача читает `ServerMessage` и эмитит `ClientEvent` в std mpsc.
//! Writer-задача читает `ClientCommand` из tokio mpsc и шлёт в сокет.

use crate::commands::{ClientCommand, ClientMode};
use crate::events::ClientEvent;
use chess_protocol::{ClientMessage, Hello, ServerMessage};
use futures::{SinkExt, StreamExt};
use std::sync::mpsc as std_mpsc;
use std::thread;
use tokio::net::TcpStream;
use tokio::sync::mpsc as tokio_mpsc;
use tokio_util::codec::{Framed, LengthDelimitedCodec};
use tracing::{info, warn};


const MAX_FRAME: usize = 1 << 26; // 64 МиБ

pub fn spawn_client(
    host: String,
    port: u16,
    hello: Hello,
) -> (
    tokio_mpsc::UnboundedSender<ClientCommand>,
    std_mpsc::Receiver<ClientEvent>,
    thread::JoinHandle<()>,
) {
    let (cmd_tx, cmd_rx) = tokio_mpsc::unbounded_channel::<ClientCommand>();
    let (ev_tx, ev_rx) = std_mpsc::channel::<ClientEvent>();

    let handle = thread::spawn(move || {
        let rt = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        {
            Ok(rt) => rt,
                               Err(e) => {
                                   let _ = ev_tx.send(ClientEvent::ConnectionLost {
                                       reason: format!("не удалось создать runtime: {e}"),
                                   });
                                   return;
                               }
        };

        rt.block_on(async move {
            if let Err(e) = run_client(host, port, hello, cmd_rx, ev_tx.clone()).await {
                let _ = ev_tx.send(ClientEvent::ConnectionLost {
                    reason: format!("{e:#}"),
                });
            }
        });
    });

    (cmd_tx, ev_rx, handle)
}

async fn run_client(
    host: String,
    port: u16,
    hello: Hello,
    mut cmd_rx: tokio_mpsc::UnboundedReceiver<ClientCommand>,
    ev_tx: std_mpsc::Sender<ClientEvent>,
) -> anyhow::Result<()> {
    let socket = TcpStream::connect((host.as_str(), port)).await?;
    let codec = LengthDelimitedCodec::builder()
    .max_frame_length(MAX_FRAME)
    .new_codec();
    let framed = Framed::new(socket, codec);

    let (mut sink, mut stream) = framed.split::<bytes::Bytes>();

    // Hello.
    let hello_msg = ClientMessage::Hello(hello);
    sink.send(serde_json::to_vec(&hello_msg)?.into()).await?;

    let _ = ev_tx.send(ClientEvent::Connected);

    // Reader.
    let ev_tx_reader = ev_tx.clone();
    let reader = tokio::spawn(async move {
        while let Some(frame) = stream.next().await {
            match frame {
                Ok(bytes) => {
                    let msg = match serde_json::from_slice::<ServerMessage>(&bytes) {
                        Ok(m) => m,
                              Err(e) => {
                                  warn!("[client] parse: {e}");
                                  continue;
                              }
                    };
                    let event = match msg {
                        ServerMessage::Welcome(w) => ClientEvent::Welcome {
                            color: w.color,
                            game_id: w.game_id,
                            opponent_name: w.opponent_name,
                            seat: w.seat,
                            partner_name: w.partner_name,
                            opponents: w.opponents,
                        },
                        ServerMessage::State(s) => ClientEvent::State(s),
                              ServerMessage::Error { message, code } => {
                                  ClientEvent::ServerError { message, code }
                              }
                              ServerMessage::GameOver { reason } => {
                                  ClientEvent::GameOver { reason }
                              }
                              ServerMessage::DrawOffer { by, by_names } => {
                                  ClientEvent::DrawOffer { by, by_names }
                              }
                              ServerMessage::DrawDeclined => ClientEvent::DrawDeclined,
                              ServerMessage::OpponentDisconnect => {
                                  ClientEvent::OpponentDisconnected
                              }
                              ServerMessage::ConnectionLost => ClientEvent::ConnectionLost {
                                  reason: "сервер закрыл соединение".into(),
                              },
                              ServerMessage::PartnerLeft { partner_name } => {
                                  ClientEvent::PartnerLeft { partner_name }
                              }
                              ServerMessage::InviteSent { to_name } => {
                                  ClientEvent::InviteSent { to_name }
                              }
                              ServerMessage::InviteReceived {
                                  from_name,
                              variant,
                              time_control,
                              } => ClientEvent::InviteReceived {
                                  from_name,
                              variant,
                              time_control,
                              },
                              ServerMessage::InviteCancelled { from_name } => {
                                  ClientEvent::InviteCancelled { from_name }
                              }
                              ServerMessage::InviteDeclined { by_name } => {
                                  ClientEvent::InviteDeclined { by_name }
                              }
                              ServerMessage::InviteError { code, message } => {
                                  ClientEvent::InviteError { code, message }
                              }
                              ServerMessage::TeamFormed {
                                  partner_name,
                              you_are,
                              } => ClientEvent::TeamFormed {
                                  partner_name,
                              you_are,
                              },
                              ServerMessage::DrawRequestToPartner { from_name } => {
                                  ClientEvent::DrawRequestToPartner { from_name }
                              }
                              ServerMessage::DrawRequestDeclined => {
                                  ClientEvent::DrawRequestDeclined
                              }
                              ServerMessage::DrawOfferAccepted { by_color, by_seat } => {
                                  ClientEvent::DrawOfferAccepted { by_color, by_seat }
                              }

                              ServerMessage::ResignRequestToPartner { from_name } => {
                                  ClientEvent::ResignRequestToPartner { from_name }
                              }


                              ServerMessage::ResignRequestToPartner { from_name } => {
                                  ClientEvent::ResignRequestToPartner { from_name }
                              }
                              ServerMessage::ResignRequestDeclined => ClientEvent::ResignRequestDeclined,
                              ServerMessage::TeamResigned { by_color } => {
                                  ClientEvent::TeamResigned { by_color }
                              }
                              // В маппинге ServerMessage -> ClientEvent:
                              ServerMessage::History { snapshots } => {
                                  ClientEvent::History { snapshots }
                              }

                    };
                    if ev_tx_reader.send(event).is_err() {
                        // UI закрылся.
                        break;
                    }
                }
                Err(e) => {
                    let _ = ev_tx_reader.send(ClientEvent::ConnectionLost {
                        reason: format!("{e}"),
                    });
                    return;
                }
            }
        }

        // EOF без ошибки.
        let _ = ev_tx_reader.send(ClientEvent::ConnectionLost {
            reason: "сервер закрыл соединение".into(),
        });
    });

    // Writer.
    while let Some(cmd) = cmd_rx.recv().await {
        let msg = match cmd {
            ClientCommand::MakeMove { from, to, promotion } => {
                ClientMessage::MakeMove { from, to, promotion }
            }
            ClientCommand::Castle { a, c } => ClientMessage::Castle { a, c },
            ClientCommand::Drop { piece, to } => ClientMessage::Drop { piece, to },
            ClientCommand::OfferDraw => ClientMessage::OfferDraw,
            ClientCommand::AcceptDraw => ClientMessage::AcceptDraw,
            ClientCommand::DeclineDraw => ClientMessage::DeclineDraw,
            ClientCommand::Resign => ClientMessage::Resign,
            ClientCommand::RequestState => ClientMessage::RequestState,
            ClientCommand::ReturnRoyal { royal_pt, to } => {
                ClientMessage::ReturnRoyal { royal_pt, to }
            }
            ClientCommand::InvitePartner { target_name } => {
                ClientMessage::InvitePartner { target_name }
            }
            ClientCommand::CancelInvite => ClientMessage::CancelInvite,
            ClientCommand::AcceptInvite { from_name } => {
                ClientMessage::AcceptInvite { from_name }
            }
            ClientCommand::DeclineInvite { from_name } => {
                ClientMessage::DeclineInvite { from_name }
            }
            ClientCommand::LeaveLobby => ClientMessage::LeaveLobby,
            ClientCommand::OfferDrawRequest => ClientMessage::OfferDrawRequest,
            ClientCommand::OfferDrawConfirm => ClientMessage::OfferDrawConfirm,
            ClientCommand::OfferDrawDecline => ClientMessage::OfferDrawDecline,
            ClientCommand::Shutdown => {
                let _ = sink.close().await;
                break;
            }
            ClientCommand::OfferResignRequest => ClientMessage::OfferResignRequest,
            ClientCommand::OfferResignConfirm => ClientMessage::OfferResignConfirm,
            ClientCommand::OfferResignDecline => ClientMessage::OfferResignDecline,
        };
        let bytes = serde_json::to_vec(&msg)?;
        if let Err(e) = sink.send(bytes.into()).await {
            let _ = ev_tx.send(ClientEvent::ConnectionLost {
                reason: format!("{e}"),
            });
            break;
        }
    }

    info!("[client] закрываюсь");
    let _ = reader.await;
    Ok(())
}

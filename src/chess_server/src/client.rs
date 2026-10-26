//! Обработка одного клиента.

use anyhow::Result;
use chess_protocol::{ClientMessage, ServerMessage};
use futures::{SinkExt, StreamExt};
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::sync::{mpsc, oneshot};
use tokio_util::codec::{Framed, LengthDelimitedCodec};
use tracing::{info, warn};

use crate::lobby::{LobbyMsg, RegisterReply};
use crate::session::SessionEvent;

const MAX_FRAME: usize = 1 << 26; // 1 МиБ

const HELLO_TIMEOUT: Duration = Duration::from_secs(5);

pub async fn handle_client(
    socket: TcpStream,
    peer: std::net::SocketAddr,
    lobby_tx: mpsc::UnboundedSender<LobbyMsg>,
) -> Result<()> {
    let codec = LengthDelimitedCodec::builder()
    .max_frame_length(MAX_FRAME)
    .new_codec();
    let framed = Framed::new(socket, codec);
    let (mut sink, mut stream) = framed.split::<bytes::Bytes>();

    // 1. Hello с таймаутом.
    let frame = match tokio::time::timeout(HELLO_TIMEOUT, stream.next()).await {
        Ok(Some(Ok(f))) => f,
        Ok(Some(Err(e))) => {
            info!("[client {peer}] ошибка чтения Hello: {e}");
            return Ok(());
        }
        Ok(None) => {
            info!("[client {peer}] соединение закрыто без Hello");
            return Ok(());
        }
        Err(_) => {
            info!("[client {peer}] таймаут ожидания Hello");
            return Ok(());
        }
    };

    let msg: ClientMessage = match serde_json::from_slice(&frame) {
        Ok(m) => m,
        Err(e) => {
            info!("[client {peer}] не удалось распарсить Hello: {e}");
            return Ok(());
        }
    };
    let hello = match msg {
        ClientMessage::Hello(h) => h,
        other => {
            info!("[client {peer}] ожидали Hello, получили {other:?}");
            return Ok(());
        }
    };

    info!(
        "[client {peer}] hello name={} variant={} time={} mode={} opponent={:?}",
        hello.name, hello.variant, hello.time_control, hello.mode, hello.opponent_name
    );

    // 2. Каналы сессии.
    let (to_session_tx, to_session_rx) = mpsc::unbounded_channel::<SessionEvent>();
    let (from_session_tx, mut from_session_rx) =
    mpsc::unbounded_channel::<ServerMessage>();

    // 3. Регистрация в лобби.
    let (reply_tx, reply_rx) = oneshot::channel::<RegisterReply>();
    lobby_tx
    .send(LobbyMsg::Register {
        name: hello.name.clone(),
          variant: hello.variant.clone(),
          time_control: hello.time_control.clone(),
          mode: hello.mode.clone(),
          opponent_name: hello.opponent_name.clone(),
          partner_name: hello.partner_name.clone(),
          to_session_rx,
          from_session_tx,
          reply_tx,
    })
    .map_err(|e| anyhow::anyhow!("lobby отвалился: {e}"))?;

    let client_id = match reply_rx.await {
        Ok(RegisterReply::Ok(id)) => id,
        Ok(RegisterReply::NameTaken) => {
            info!("[client {peer}] имя занято, закрываю");
            return Ok(());
        }
        Err(_) => {
            info!("[client {peer}] лобби не ответило id");
            return Ok(());
        }
    };

    info!("[client {peer}] зарегистрирован как client {client_id}");

    // 4. Writer.
    let peer_w = peer;
    let writer = tokio::spawn(async move {
        while let Some(msg) = from_session_rx.recv().await {
            let bytes = match serde_json::to_vec(&msg) {
                Ok(b) => b,
                              Err(e) => {
                                  warn!("[client {peer_w}] serialize: {e}");
                                  continue;
                              }
            };
            if sink.send(bytes.into()).await.is_err() {
                // Соединение закрыто на запись — writer больше не нужен.
                // Reader продолжит читать, пока клиент не отключится,
                // и пошлёт SessionEvent::Disconnected в сессию.
                break;
            }
        }
    });

    // 5. Reader: разделяем лобби-команды и команды сессии.
    let peer_r = peer;
    let lobby_tx_r = lobby_tx.clone();
    let reader = tokio::spawn(async move {
        while let Some(frame) = stream.next().await {
            let bytes = match frame {
                Ok(b) => b,
                              Err(e) => {
                                  info!("[client {peer_r}] socket: {e}");
                                  break;
                              }
            };
            let msg: ClientMessage = match serde_json::from_slice(&bytes) {
                Ok(m) => m,
                              Err(e) => {
                                  info!("[client {peer_r}] parse: {e}");
                                  continue;
                              }
            };
            info!("[client {peer_r}] получено: {:?}", msg);

            // === Лобби-команды: идут в lobby_tx, НЕ в сессию ===
            let lobby_msg: Option<LobbyMsg> = match msg {
                ClientMessage::InvitePartner { target_name } => {
                    Some(LobbyMsg::Invite { from: client_id, target_name })
                }
                ClientMessage::CancelInvite => {
                    Some(LobbyMsg::CancelInvite { from: client_id })
                }
                ClientMessage::AcceptInvite { from_name } => {
                    Some(LobbyMsg::AcceptInvite { from: client_id, inviter_name: from_name })
                }
                ClientMessage::DeclineInvite { from_name } => {
                    Some(LobbyMsg::DeclineInvite { from: client_id, inviter_name: from_name })
                }
                ClientMessage::LeaveLobby => {
                    Some(LobbyMsg::LeaveLobby { id: client_id })
                }
                // Всё остальное — в сессию.
                other => {
                    if to_session_tx.send(SessionEvent::FromClient(other)).is_err() {
                        break;
                    }
                    None
                }
            };

            if let Some(lm) = lobby_msg {
                if lobby_tx_r.send(lm).is_err() {
                    info!("[client {peer_r}] lobby отвалился");
                    break;
                }
            }
        }
        let _ = to_session_tx.send(SessionEvent::Disconnected);
    });

    // 6. Ждём завершения.
    //
    // Ключевой момент: если writer умер (например, FrameTooLong или
    // соединение закрыто на запись), reader продолжает читать и в конце
    // пошлёт SessionEvent::Disconnected. Если завершиться сразу по writer'у,
    // from_session_rx дропнется, сессия продолжит жить, но не сможет
    // доставлять State — клиенты «зависнут».
    //
    // Поэтому: при смерти writer'а ДОЖИДАЕМСЯ reader'а.
    let mut reader = reader;
    let mut writer = writer;
    tokio::select! {
        _ = &mut reader => {
            info!("[client {peer}] reader завершился");
            writer.abort();
            // Reader завершился первым — writer дропнет from_session_rx
            // сам, когда сессия закроет tx, или writer уже завершён.
            let _ = writer.await;
        }
        _ = &mut writer => {
            info!("[client {peer}] writer завершился, ждём reader");
            // Дожидаемся reader'а, чтобы он успел послать
            // SessionEvent::Disconnected в сессию.
            let _ = reader.await;
        }
    }

    info!("[client {peer}] завершение, уведомляю лобби (client {client_id})");
    let _ = lobby_tx.send(LobbyMsg::ClientGone { id: client_id });

    Ok(())
}

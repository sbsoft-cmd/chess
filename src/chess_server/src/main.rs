//! Шахматный сервер: TCP listener + lobby + sessions.

mod client;
mod lobby;
mod session;

use anyhow::Result;
use tokio::net::TcpListener;
use tracing::info;
use tracing_subscriber::EnvFilter;

use crate::lobby::{lobby_task, LobbyMsg};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
    .with_env_filter(
        EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
    )
    .init();

    let (lobby_tx, lobby_rx) = tokio::sync::mpsc::unbounded_channel::<LobbyMsg>();
    tokio::spawn(lobby_task(lobby_rx));

    let addr = "0.0.0.0:8080";
    let listener = TcpListener::bind(addr).await?;
    info!("[server] слушаю {addr}");
    info!("[server] Ctrl+C для завершения");

    loop {
        tokio::select! {
            // Ctrl+C — graceful shutdown.
            _ = tokio::signal::ctrl_c() => {
                info!("[server] Ctrl+C, завершаюсь");
                return Ok(());
            }

            // Новое соединение.
            res = listener.accept() => {
                match res {
                    Ok((socket, peer)) => {
                        info!("[server] новое соединение {peer}");
                        let lobby_tx = lobby_tx.clone();
                        tokio::spawn(async move {
                            if let Err(e) = client::handle_client(socket, peer, lobby_tx).await {
                                info!("[server] клиент {peer}: {e:?}");
                            }
                        });
                    }
                    Err(e) => {
                        tracing::warn!("[server] accept: {e}");
                        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                    }
                }
            }
        }
    }
}

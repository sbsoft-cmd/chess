//! Сетевой клиент: общий для CLI и GUI.

mod commands;
mod events;
mod run;

pub use commands::{ClientCommand, ClientMode, seat_of};
pub use events::ClientEvent;
pub use run::spawn_client;

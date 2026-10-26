//! Точка входа UI.

mod app;
mod assets;
mod layout;
mod setup;
mod text;


use anyhow::Result;
use tracing_subscriber::EnvFilter;
use winit::event_loop::EventLoop;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
    .with_env_filter(
        EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
    )
    .init();

    let event_loop = EventLoop::new()?;
    let mut app = app::App::new();
    event_loop.run_app(&mut app)?;
    Ok(())
}

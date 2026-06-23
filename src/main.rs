#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

mod app;
mod font;
mod renderer;
mod text;

use anyhow::{Context as _, Result};
use app::App;
use tracing_subscriber::filter::LevelFilter;
use winit::event_loop::EventLoop;

/// Initializes logging, creates the event loop, and starts the application.
fn main() -> Result<()> {
    init_tracing();

    tracing::trace!("starting harbor");
    let event_loop = EventLoop::new().context("create event loop")?;

    let mut app = App::default();
    event_loop.run_app(&mut app).context("run event loop")?;

    Ok(())
}

/// Configures the log level for the current build mode.
fn init_tracing() {
    tracing_subscriber::fmt().with_max_level(log_level()).init();
}

#[cfg(debug_assertions)]
/// Debug builds emit detailed logs for window and render events.
fn log_level() -> LevelFilter {
    LevelFilter::TRACE
}

#[cfg(not(debug_assertions))]
/// Release builds keep only warnings and errors.
fn log_level() -> LevelFilter {
    LevelFilter::WARN
}

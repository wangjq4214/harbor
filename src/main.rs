#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

mod app;
mod cursor;
mod font;
mod gpu;
mod metrics;
mod pty;
mod render;
mod renderer;
mod terminal;

use anyhow::{Context as _, Result};
use app::{App, AppEvent};
use tracing_subscriber::filter::LevelFilter;
use winit::event_loop::EventLoop;

/// Creates the event loop and starts the application.
fn main() -> Result<()> {
    init_tracing();
    tracing::trace!("starting harbor");

    let event_loop = EventLoop::<AppEvent>::with_user_event()
        .build()
        .context("create event loop")?;
    let mut app = App::new(event_loop.create_proxy());
    event_loop.run_app(&mut app).context("run event loop")?;

    Ok(())
}

/// Configures the log level for the current build mode.
fn init_tracing() {
    tracing_subscriber::fmt().with_max_level(log_level()).init();
}

#[cfg(debug_assertions)]
/// Debug builds keep dependency startup logs quiet; wgpu/naga DEBUG output makes launch visibly slow.
fn log_level() -> LevelFilter {
    LevelFilter::INFO
}

#[cfg(not(debug_assertions))]
/// Release builds keep only warnings and errors.
fn log_level() -> LevelFilter {
    LevelFilter::WARN
}

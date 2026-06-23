#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

mod app;
mod renderer;

use anyhow::{Context as _, Result};
use app::App;
use tracing_subscriber::filter::LevelFilter;
use winit::event_loop::EventLoop;

fn main() -> Result<()> {
    init_tracing();

    tracing::trace!("starting harbor");
    let event_loop = EventLoop::new().context("create event loop")?;

    let mut app = App::default();
    event_loop.run_app(&mut app).context("run event loop")?;

    Ok(())
}

fn init_tracing() {
    tracing_subscriber::fmt().with_max_level(log_level()).init();
}

#[cfg(debug_assertions)]
fn log_level() -> LevelFilter {
    LevelFilter::TRACE
}

#[cfg(not(debug_assertions))]
fn log_level() -> LevelFilter {
    LevelFilter::WARN
}

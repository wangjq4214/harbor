#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

mod app;
mod config;
mod pty;
mod render;
mod terminal;

use anyhow::{Context as _, Result};
use app::{App, AppEvent};
use std::path::PathBuf;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::filter::{EnvFilter, LevelFilter};
use winit::event_loop::EventLoop;

/// Creates the event loop and starts the application.
fn main() -> Result<()> {
    let _guard = init_tracing();
    tracing::trace!("starting harbor");

    let event_loop = EventLoop::<AppEvent>::with_user_event()
        .build()
        .context("create event loop")?;
    let mut app = App::new(event_loop.create_proxy());
    event_loop.run_app(&mut app).context("run event loop")?;

    Ok(())
}

/// Log directory: debug → cwd, release → ~/.harbor/.
fn log_dir() -> PathBuf {
    #[cfg(debug_assertions)]
    {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    }
    #[cfg(not(debug_assertions))]
    {
        let home = dirs::home_dir().expect("home directory not found");
        let dir = home.join(".harbor");
        std::fs::create_dir_all(&dir).expect("failed to create .harbor directory");
        dir
    }
}

/// Configures tracing: respects `RUST_LOG` env var, falls back to compile-time level.
/// Writes to a file instead of stderr.
fn init_tracing() -> WorkerGuard {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(log_level().to_string()));

    let log_dir = log_dir();
    let file_appender = tracing_appender::rolling::never(&log_dir, "harbor.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    tracing_subscriber::fmt()
        .json()
        .with_env_filter(filter)
        .with_writer(non_blocking)
        .init();

    tracing::info!(log_dir = %log_dir.display(), "tracing initialized");
    guard
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

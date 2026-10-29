//! Application models and widget composition shared by the Runtime Host and debug UI library.

#[cfg(test)]
#[path = "../../../tests/support/gpu.rs"]
mod test_gpu;

mod appearance;
pub mod command;
pub mod confirmation;
pub mod notifications;
mod popups;
mod search;
pub use search::SearchCommandOutcome;
pub mod tab_manager;
pub mod tab_view;
mod terminal_commands;
pub mod terminal_view;
pub mod workspace;

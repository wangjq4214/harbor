//! Application models and widget composition shared by the Runtime Host and debug UI library.

mod appearance;
pub mod command;
pub mod confirmation;
pub mod notifications;
mod search;
pub use search::SearchCommandOutcome;
pub mod tab_manager;
pub mod tab_view;
mod terminal_commands;
pub mod terminal_view;
pub mod workspace;

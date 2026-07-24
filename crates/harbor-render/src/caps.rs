use harbor_types::{InputRequest, SelectionBounds};
use std::time::Instant;

use crate::EventResult;

/// Concrete requests produced by interactive renderer components.
#[derive(Debug)]
pub enum UiRequest {
    Copy(SelectionBounds),
    Input(InputRequest),
    Paste(String),
    Scroll(isize),
    ScrollToTop,
    ScrollToBottom,
    SetSelectionDragActive(bool),
    Redraw,
}

/// A component event outcome and the concrete requests needed to enact it.
#[derive(Debug)]
pub struct InteractionResult {
    pub event: EventResult,
    pub requests: Vec<UiRequest>,
}

impl InteractionResult {
    pub fn continue_() -> Self {
        Self {
            event: EventResult::Continue,
            requests: Vec::new(),
        }
    }

    pub fn handled() -> Self {
        Self {
            event: EventResult::Handled,
            requests: Vec::new(),
        }
    }

    pub fn with_request(event: EventResult, request: UiRequest) -> Self {
        Self {
            event,
            requests: vec![request],
        }
    }
}

/// A timer result with the earliest requested wake deadline and side effects.
#[derive(Debug, Default)]
pub struct WaitResult {
    pub deadline: Option<Instant>,
    pub requests: Vec<UiRequest>,
}

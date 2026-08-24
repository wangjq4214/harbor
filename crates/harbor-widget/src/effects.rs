//! Platform-neutral effects produced by the widget runtime.
//!
//! These values describe requests for a host to apply. They deliberately do
//! not contain window-system or application-specific types.

use crate::layout::Point;
use std::time::Instant;

/// A requested event-loop waiting mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlFlowEffect {
    /// Wait until the host receives another event.
    Wait,
    /// Wait until the supplied deadline.
    WaitUntil(Instant),
    /// Continue processing events without waiting.
    Poll,
}

impl ControlFlowEffect {
    pub const fn wait() -> Self {
        Self::Wait
    }

    pub const fn wait_until(deadline: Instant) -> Self {
        Self::WaitUntil(deadline)
    }

    pub const fn poll() -> Self {
        Self::Poll
    }

    /// Combines two independent window wait requests.
    ///
    /// `Poll` dominates, otherwise the earliest `WaitUntil` dominates `Wait`.
    /// This is distinct from sequential [`RuntimeEffects::merge`], which keeps
    /// the later turn's control-flow request.
    pub fn arbitrate(self, other: Self) -> Self {
        match (self, other) {
            (Self::Poll, _) | (_, Self::Poll) => Self::Poll,
            (Self::WaitUntil(left), Self::WaitUntil(right)) => Self::WaitUntil(left.min(right)),
            (Self::WaitUntil(deadline), Self::Wait) | (Self::Wait, Self::WaitUntil(deadline)) => {
                Self::WaitUntil(deadline)
            }
            (Self::Wait, Self::Wait) => Self::Wait,
        }
    }
}

/// Platform-neutral cursor shapes understood by the host adapter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CursorShape {
    Default,
    Pointer,
    Text,
    Crosshair,
    Grab,
    Grabbing,
    NotAllowed,
    ResizeHorizontal,
    ResizeVertical,
}

/// A cursor selection or reset request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CursorEffect {
    Set(CursorShape),
    Reset,
}

impl CursorEffect {
    pub const fn set_cursor(shape: CursorShape) -> Self {
        Self::Set(shape)
    }

    pub const fn reset() -> Self {
        Self::Reset
    }
}

/// A platform-neutral IME allowance and candidate-position request.
///
/// Allowance and position are independent operations, so a single effect can
/// carry both when separate requests are merged into one runtime turn.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ImeEffect {
    pub allowed: Option<bool>,
    pub position: Option<Point>,
}

impl ImeEffect {
    pub const fn set_allowed(allowed: bool) -> Self {
        Self {
            allowed: Some(allowed),
            position: None,
        }
    }

    pub const fn set_position(position: Point) -> Self {
        Self {
            allowed: None,
            position: Some(position),
        }
    }

    fn merge(&mut self, other: Self) {
        self.allowed = other.allowed.or(self.allowed);
        self.position = other.position.or(self.position);
    }
}

/// A clipboard operation for the host to perform.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClipboardEffect {
    Read,
    Write(String),
}

impl ClipboardEffect {
    pub const fn read() -> Self {
        Self::Read
    }

    pub fn write(contents: impl Into<String>) -> Self {
        Self::Write(contents.into())
    }
}

/// A source-agnostic notification that host-owned work changed UI output.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ExternalInvalidation;

impl ExternalInvalidation {
    pub const fn new() -> Self {
        Self
    }
}

/// The mergeable batch of requests produced by one runtime turn.
#[derive(Clone, Debug, PartialEq)]
pub struct RuntimeEffects {
    /// Whether the host should schedule a redraw.
    pub request_redraw: bool,
    /// The latest requested event-loop mode, if any.
    pub control_flow: Option<ControlFlowEffect>,
    /// The latest cursor operation, if any.
    pub cursor: Option<CursorEffect>,
    /// The latest IME operation, if any.
    pub ime: Option<ImeEffect>,
    /// The latest clipboard operation, if any.
    pub clipboard: Option<ClipboardEffect>,
    /// Window turns stay eligible. Per-draw deferral is an encode-mode choice.
    pub ordinary_present_eligible: bool,
    /// True when this encode may live-commit ineligible externals.
    pub force_present: bool,
    /// True when at least one external schedule provider is ineligible this turn.
    pub has_deferred_externals: bool,
}

impl Default for RuntimeEffects {
    fn default() -> Self {
        Self {
            request_redraw: false,
            control_flow: None,
            cursor: None,
            ime: None,
            clipboard: None,
            ordinary_present_eligible: true,
            force_present: false,
            has_deferred_externals: false,
        }
    }
}

impl RuntimeEffects {
    /// Creates an effect batch containing only a redraw request.
    pub const fn request_redraw() -> Self {
        Self {
            request_redraw: true,
            control_flow: None,
            cursor: None,
            ime: None,
            clipboard: None,
            ordinary_present_eligible: true,
            force_present: false,
            has_deferred_externals: false,
        }
    }

    /// Creates an effect batch from the current redraw decision.
    pub const fn from_redraw(request_redraw: bool) -> Self {
        Self {
            request_redraw,
            control_flow: None,
            cursor: None,
            ime: None,
            clipboard: None,
            ordinary_present_eligible: true,
            force_present: false,
            has_deferred_externals: false,
        }
    }

    /// Creates an effect batch that live-commits deferred externals this pass.
    pub const fn force_present() -> Self {
        Self {
            request_redraw: true,
            control_flow: None,
            cursor: None,
            ime: None,
            clipboard: None,
            ordinary_present_eligible: true,
            force_present: true,
            has_deferred_externals: false,
        }
    }

    /// Merges another batch into this one.
    ///
    /// Redraw requests are coalesced. Ordinary-present eligibility is AND-ed so a
    /// deferred provider cannot be cleared by a later default batch. Force and
    /// deferred-external presence are OR-ed. For each optional operation, the
    /// later request wins while an earlier request is retained when the later
    /// batch has no request of that kind.
    pub fn merge(&mut self, other: Self) {
        self.request_redraw |= other.request_redraw;
        self.ordinary_present_eligible &= other.ordinary_present_eligible;
        self.force_present |= other.force_present;
        self.has_deferred_externals |= other.has_deferred_externals;
        self.control_flow = other.control_flow.or(self.control_flow);
        self.cursor = other.cursor.or(self.cursor);
        if let Some(other_ime) = other.ime {
            if let Some(ime) = self.ime.as_mut() {
                ime.merge(other_ime);
            } else {
                self.ime = Some(other_ime);
            }
        }
        self.clipboard = other.clipboard.or_else(|| self.clipboard.clone());
    }

    /// Returns a merged copy without consuming either input.
    pub fn merged(mut self, other: &Self) -> Self {
        self.merge(other.clone());
        self
    }

    pub fn is_noop(&self) -> bool {
        !self.request_redraw
            && self.control_flow.is_none()
            && self.cursor.is_none()
            && self.ime.is_none()
            && self.clipboard.is_none()
            && self.ordinary_present_eligible
            && !self.force_present
            && !self.has_deferred_externals
    }
}

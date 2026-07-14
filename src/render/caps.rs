//! Capability traits for interactive UI layers.
//!
//! Access traits describe what a handler may touch. Each layer context
//! implements only the access traits it is allowed to grant. Layer handlers
//! are bounded on exactly those traits — a missing bound is a compile error,
//! and an over-broad context would have to implement rights it should not have.

use crate::{pty::Pty, terminal::Terminal};
use std::time::Instant;
use winit::event::WindowEvent;
use winit::keyboard::ModifiersState;
use winit::window::Window;

use super::{EventResult, GpuContext};

// ── Access traits (resources a handler may request) ─────────────────────────

pub(crate) trait TerminalAccess {
    fn terminal(&self) -> &Terminal;
}

/// Redraw-only window right. Prefer this over handing out a full `&Window`.
pub(crate) trait RedrawAccess {
    fn request_redraw(&self);
}

pub(crate) trait GpuAccess {
    fn gpu(&self) -> &GpuContext;
}

pub(crate) trait PtyAccess {
    fn pty(&mut self) -> &mut Pty;
}

pub(crate) trait ModifiersAccess {
    fn modifiers(&self) -> ModifiersState;
}

/// Scroll and auto-scroll control for selection drag.
pub(crate) trait ScrollAccess {
    fn scroll_viewport_up(&mut self, n: usize);
    fn scroll_viewport_down(&mut self, n: usize);
    /// Suppress PTY-output scroll-to-bottom snap during drag.
    fn set_auto_scrolling(&mut self, active: bool);
}

// ── Layer contexts (grant only their own rights) ────────────────────────────

pub(crate) struct SelectionContext<'a> {
    pub(crate) terminal: &'a mut Terminal,
    pub(crate) window: &'a Window,
    pub(crate) pty: &'a mut Pty,
    pub(crate) modifiers: ModifiersState,
}

impl TerminalAccess for SelectionContext<'_> {
    fn terminal(&self) -> &Terminal {
        self.terminal
    }
}

impl RedrawAccess for SelectionContext<'_> {
    fn request_redraw(&self) {
        self.window.request_redraw();
    }
}

impl PtyAccess for SelectionContext<'_> {
    fn pty(&mut self) -> &mut Pty {
        self.pty
    }
}

impl ModifiersAccess for SelectionContext<'_> {
    fn modifiers(&self) -> ModifiersState {
        self.modifiers
    }
}

impl ScrollAccess for SelectionContext<'_> {
    fn scroll_viewport_up(&mut self, n: usize) {
        self.terminal.scroll_viewport_up(n);
    }
    fn scroll_viewport_down(&mut self, n: usize) {
        self.terminal.scroll_viewport_down(n);
    }
    fn set_auto_scrolling(&mut self, active: bool) {
        self.terminal.set_suppress_scroll_snap(active);
    }
}

// ── SelectionWaitContext ──────────────────────────────────────

/// Timer context for selection auto-scroll — grants scroll + read + redraw.
pub(crate) struct SelectionWaitContext<'a> {
    pub(crate) terminal: &'a mut Terminal,
    pub(crate) window: &'a Window,
}

impl TerminalAccess for SelectionWaitContext<'_> {
    fn terminal(&self) -> &Terminal {
        self.terminal
    }
}

impl RedrawAccess for SelectionWaitContext<'_> {
    fn request_redraw(&self) {
        self.window.request_redraw();
    }
}

impl ScrollAccess for SelectionWaitContext<'_> {
    fn scroll_viewport_up(&mut self, n: usize) {
        self.terminal.scroll_viewport_up(n);
    }

    fn scroll_viewport_down(&mut self, n: usize) {
        self.terminal.scroll_viewport_down(n);
    }

    fn set_auto_scrolling(&mut self, active: bool) {
        self.terminal.set_suppress_scroll_snap(active);
    }
}

pub(crate) struct ScrollbarContext<'a> {
    pub(crate) terminal: &'a Terminal,
    pub(crate) gpu: &'a GpuContext,
    pub(crate) window: &'a Window,
}

impl TerminalAccess for ScrollbarContext<'_> {
    fn terminal(&self) -> &Terminal {
        self.terminal
    }
}
impl GpuAccess for ScrollbarContext<'_> {
    fn gpu(&self) -> &GpuContext {
        self.gpu
    }
}
impl RedrawAccess for ScrollbarContext<'_> {
    fn request_redraw(&self) {
        self.window.request_redraw();
    }
}

/// Auto-hide timer only needs redraw.
pub(crate) struct ScrollbarWaitContext<'a> {
    pub(crate) window: &'a Window,
}

impl RedrawAccess for ScrollbarWaitContext<'_> {
    fn request_redraw(&self) {
        self.window.request_redraw();
    }
}

pub(crate) struct CursorContext<'a> {
    pub(crate) terminal: &'a Terminal,
    pub(crate) gpu: &'a GpuContext,
}

impl TerminalAccess for CursorContext<'_> {
    fn terminal(&self) -> &Terminal {
        self.terminal
    }
}
impl GpuAccess for CursorContext<'_> {
    fn gpu(&self) -> &GpuContext {
        self.gpu
    }
}

pub(crate) struct CursorWaitContext<'a> {
    pub(crate) terminal: &'a Terminal,
    pub(crate) window: &'a Window,
}

impl TerminalAccess for CursorWaitContext<'_> {
    fn terminal(&self) -> &Terminal {
        self.terminal
    }
}

impl RedrawAccess for CursorWaitContext<'_> {
    fn request_redraw(&self) {
        self.window.request_redraw();
    }
}

// ── Layer handler traits (exact rights, no more) ────────────────────────────

/// Selection: terminal + redraw + PTY write + modifiers + scroll.
pub(crate) trait SelectionInput {
    fn handle_event<C>(&mut self, event: &WindowEvent, caps: &mut C) -> EventResult
    where
        C: TerminalAccess + RedrawAccess + PtyAccess + ModifiersAccess + ScrollAccess;

    fn on_about_to_wait<C>(&mut self, caps: &mut C) -> Option<Instant>
    where
        C: TerminalAccess + ScrollAccess + RedrawAccess;
}

/// Scrollbar: terminal + gpu + redraw on events; redraw only for auto-hide.
pub(crate) trait ScrollbarInput {
    fn handle_event<C>(&mut self, event: &WindowEvent, caps: &C) -> EventResult
    where
        C: TerminalAccess + GpuAccess + RedrawAccess;

    fn on_about_to_wait<C>(&mut self, caps: &C) -> Option<Instant>
    where
        C: RedrawAccess;
}

/// Cursor: terminal + gpu on event; terminal + redraw on blink timer.
pub(crate) trait CursorInput {
    fn handle_event<C>(&mut self, event: &WindowEvent, caps: &C) -> EventResult
    where
        C: TerminalAccess + GpuAccess;

    fn on_about_to_wait<C>(&mut self, caps: &C) -> Option<Instant>
    where
        C: TerminalAccess + RedrawAccess;
}

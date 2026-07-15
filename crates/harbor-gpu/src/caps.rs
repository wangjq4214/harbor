//! Capability traits for interactive UI layers.
//!
//! Access traits describe what a handler may touch. Each layer context
//! implements only the access traits it is allowed to grant. Layer handlers
//! are bounded on exactly those traits — a missing bound is a compile error,
//! and an over-broad context would have to implement rights it should not have.

use harbor_pty::Pty;
use harbor_terminal::Terminal;
use std::time::Instant;
use winit::event::WindowEvent;
use winit::keyboard::ModifiersState;
use winit::window::Window;

use crate::{EventResult, GpuContext};

// ── Access traits (resources a handler may request) ─────────────────────────

pub trait TerminalAccess {
    fn terminal(&self) -> &Terminal;
}

/// Redraw-only window right. Prefer this over handing out a full `&Window`.
pub trait RedrawAccess {
    fn request_redraw(&self);
}

pub trait GpuAccess {
    fn gpu(&self) -> &GpuContext;
}

pub trait PtyAccess {
    fn pty(&mut self) -> &mut Pty;
}

pub trait ModifiersAccess {
    fn modifiers(&self) -> ModifiersState;
}

/// Scroll and auto-scroll control for selection drag.
pub trait ScrollAccess {
    fn scroll_viewport_up(&mut self, n: usize);
    fn scroll_viewport_down(&mut self, n: usize);
    /// Suppress PTY-output scroll-to-bottom snap during drag.
    fn set_auto_scrolling(&mut self, active: bool);
}

// ── Layer contexts (grant only their own rights) ────────────────────────────

pub struct SelectionContext<'a> {
    pub terminal: &'a mut Terminal,
    pub window: &'a Window,
    pub pty: &'a mut Pty,
    pub modifiers: ModifiersState,
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
pub struct SelectionWaitContext<'a> {
    pub terminal: &'a mut Terminal,
    pub window: &'a Window,
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

pub struct ScrollbarContext<'a> {
    pub terminal: &'a Terminal,
    pub gpu: &'a GpuContext,
    pub window: &'a Window,
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
pub struct ScrollbarWaitContext<'a> {
    pub window: &'a Window,
}

impl RedrawAccess for ScrollbarWaitContext<'_> {
    fn request_redraw(&self) {
        self.window.request_redraw();
    }
}

pub struct CursorContext<'a> {
    pub terminal: &'a Terminal,
    pub gpu: &'a GpuContext,
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

pub struct CursorWaitContext<'a> {
    pub terminal: &'a Terminal,
    pub window: &'a Window,
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
pub trait SelectionInput {
    fn handle_event<C>(&mut self, event: &WindowEvent, caps: &mut C) -> EventResult
    where
        C: TerminalAccess + RedrawAccess + PtyAccess + ModifiersAccess + ScrollAccess;

    fn on_about_to_wait<C>(&mut self, caps: &mut C) -> Option<Instant>
    where
        C: TerminalAccess + ScrollAccess + RedrawAccess;
}

/// Scrollbar: terminal + gpu + redraw on events; redraw only for auto-hide.
pub trait ScrollbarInput {
    fn handle_event<C>(&mut self, event: &WindowEvent, caps: &C) -> EventResult
    where
        C: TerminalAccess + GpuAccess + RedrawAccess;

    fn on_about_to_wait<C>(&mut self, caps: &C) -> Option<Instant>
    where
        C: RedrawAccess;
}

/// Cursor: terminal + gpu on event; terminal + redraw on blink timer.
pub trait CursorInput {
    fn handle_event<C>(&mut self, event: &WindowEvent, caps: &C) -> EventResult
    where
        C: TerminalAccess + GpuAccess;

    fn on_about_to_wait<C>(&mut self, caps: &C) -> Option<Instant>
    where
        C: TerminalAccess + RedrawAccess;
}

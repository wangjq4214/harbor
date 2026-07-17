use std::time::{Duration, Instant};

use harbor_config::SCROLLBAR_HIDE_DELAY_MS;

use super::{
    EventResult,
    caps::{RedrawAccess, ScrollbarInput, TerminalAccess},
};

/// Shell-owned scrollbar visibility state. Rendering belongs to the terminal widget.
pub struct Scrollbar {
    visible: bool,
    cursor_inside: bool,
    last_activity: Instant,
}

impl Scrollbar {
    pub fn new() -> Self {
        Self {
            visible: false,
            cursor_inside: false,
            last_activity: Instant::now(),
        }
    }

    pub(crate) const fn is_visible(&self) -> bool {
        self.visible
    }

    fn show(&mut self) {
        self.visible = true;
        self.last_activity = Instant::now();
    }
}

impl ScrollbarInput for Scrollbar {
    fn handle_event<C>(&mut self, event: &winit::event::WindowEvent, caps: &C) -> EventResult
    where
        C: TerminalAccess + RedrawAccess,
    {
        match event {
            winit::event::WindowEvent::CursorEntered { .. } => {
                self.cursor_inside = true;
                self.show();
                caps.request_redraw();
                EventResult::Handled
            }
            winit::event::WindowEvent::CursorMoved { .. } => {
                self.last_activity = Instant::now();
                if !self.visible && self.cursor_inside {
                    self.show();
                    caps.request_redraw();
                }
                EventResult::Handled
            }
            winit::event::WindowEvent::CursorLeft { .. } => {
                self.cursor_inside = false;
                EventResult::Continue
            }
            winit::event::WindowEvent::MouseWheel { .. } => {
                self.last_activity = Instant::now();
                if !self.visible {
                    self.show();
                    caps.request_redraw();
                }
                EventResult::Continue
            }
            _ => EventResult::Continue,
        }
    }

    fn on_about_to_wait<C>(&mut self, caps: &C) -> Option<Instant>
    where
        C: RedrawAccess,
    {
        if !self.visible {
            return None;
        }
        let hide_delay = Duration::from_millis(SCROLLBAR_HIDE_DELAY_MS);
        if self.last_activity.elapsed() >= hide_delay {
            self.visible = false;
            caps.request_redraw();
            None
        } else {
            Some(self.last_activity + hide_delay)
        }
    }
}

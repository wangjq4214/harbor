use std::time::{Duration, Instant};

use harbor_config::BLINK_INTERVAL_MS;
use harbor_types::RenderSnapshot;

use super::{
    EventResult,
    caps::{CursorInput, RedrawAccess, TerminalAccess},
};

fn should_render_cursor(snapshot: &RenderSnapshot, blink_visible: bool) -> bool {
    snapshot.cursor_visible && (!snapshot.cursor_blink || blink_visible)
}

/// Shell-owned cursor blink state. Rendering belongs to the terminal widget.
pub struct Cursor {
    visible: bool,
    blink_start: Instant,
    last_rendered_visible: bool,
}

impl Cursor {
    pub fn new() -> Self {
        Self {
            visible: false,
            blink_start: Instant::now(),
            last_rendered_visible: false,
        }
    }

    pub(crate) const fn is_visible(&self) -> bool {
        self.visible
    }

    fn blink_visible(&self) -> bool {
        let millis = self.blink_start.elapsed().as_millis() as u64;
        (millis / BLINK_INTERVAL_MS).is_multiple_of(2)
    }
}

impl CursorInput for Cursor {
    fn handle_event<C>(&mut self, event: &winit::event::WindowEvent, caps: &C) -> EventResult
    where
        C: TerminalAccess,
    {
        if matches!(event, winit::event::WindowEvent::RedrawRequested) {
            let snapshot = caps.terminal().screen().snapshot();
            self.visible = should_render_cursor(&snapshot, self.blink_visible());
            self.last_rendered_visible = self.blink_visible();
        }
        EventResult::Continue
    }

    fn on_about_to_wait<C>(&mut self, caps: &C) -> Option<Instant>
    where
        C: TerminalAccess + RedrawAccess,
    {
        let screen = caps.terminal().screen();
        if !screen.cursor_visible() || !screen.cursor_blink() {
            return None;
        }
        let visible = self.blink_visible();
        if visible != self.last_rendered_visible {
            caps.request_redraw();
        }
        let millis = self.blink_start.elapsed().as_millis() as u64;
        let next_toggle_ms = ((millis / BLINK_INTERVAL_MS) + 1) * BLINK_INTERVAL_MS;
        Some(self.blink_start + Duration::from_millis(next_toggle_ms))
    }
}

#[cfg(test)]
mod tests {
    use super::should_render_cursor;
    use harbor_terminal::Terminal;

    #[test]
    fn dectcem_controls_rendered_cursor_visibility() {
        let mut terminal = Terminal::new(3, 3);
        assert!(should_render_cursor(&terminal.screen().snapshot(), true));
        assert!(!should_render_cursor(&terminal.screen().snapshot(), false));

        terminal.put_bytes(b"\x1b[2 q");
        assert!(should_render_cursor(&terminal.screen().snapshot(), false));

        terminal.put_bytes(b"\x1b[?25l");
        assert!(!should_render_cursor(&terminal.screen().snapshot(), true));

        terminal.put_bytes(b"\x1b[?25h");
        assert!(should_render_cursor(&terminal.screen().snapshot(), true));
    }
}

use std::sync::Arc;

use winit::{
    application::ApplicationHandler,
    event::{ElementState, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoopProxy},
    keyboard::{Key, ModifiersState, NamedKey},
    window::{Window, WindowId},
};

use crate::{
    cursor::CursorBlink, pty::Pty, renderer::Renderer, terminal::Terminal, terminal::TerminalSize,
};

/// Events posted back to the winit event loop from background workers.
pub(crate) enum AppEvent {
    /// Raw bytes read from the shell process and decoded by the renderer.
    PtyOutput(Vec<u8>),
}

/// Application state holding the window and its renderer.
pub(crate) struct App {
    /// The primary window, wrapped in `Arc` so the renderer can share ownership.
    window: Option<Arc<Window>>,
    /// Handles all drawing to the window surface.
    renderer: Option<Renderer>,
    /// Terminal model: byte-stream parser plus visible screen.
    terminal: Option<Terminal>,
    /// Shell process with background output reader.
    pty: Pty,
    cursor_blink: CursorBlink,
    /// Coalesced pending resize; applied in `about_to_wait` to avoid bounce.
    pending_resize: Option<TerminalSize>,
    /// Currently active keyboard modifiers (tracked via `ModifiersChanged`).
    modifiers: ModifiersState,
}

/// Errors that can occur while starting the application.
#[derive(Debug, thiserror::Error)]
enum AppError {
    #[error("failed to create window")]
    Window(#[from] winit::error::OsError),
    #[error("failed to create renderer")]
    Renderer(#[source] anyhow::Error),
    #[error("failed to start shell pty")]
    Pty(#[source] anyhow::Error),
}

// ── ApplicationHandler (winit lifecycle) ──────────────────────────────────

/// Handles the winit lifecycle and window events.
impl ApplicationHandler<AppEvent> for App {
    /// Called on start or wake from suspend.  Bootstraps the window, renderer,
    /// terminal, and PTY on first call; no-ops on repeated resumes to keep
    /// existing state intact.
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if let Err(error) = self.try_resume(event_loop) {
            tracing::error!(error = %format_args!("{error:#}"), "application error");
            event_loop.exit();
        }
    }

    /// Handles custom events posted from background workers.
    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: AppEvent) {
        match event {
            // PTY reader thread sent decoded bytes — feed them to the terminal parser.
            AppEvent::PtyOutput(output) => {
                if let (Some(renderer), Some(window), Some(terminal)) = (
                    self.renderer.as_mut(),
                    self.window.as_ref(),
                    self.terminal.as_mut(),
                ) {
                    terminal.process_output(renderer, window, &output);
                }
            }
        }
    }

    /// Called when the event loop is about to block.  Drives cursor blink:
    /// gated by the screen's `cursor_blink` flag.  When blinking is off,
    /// uses `ControlFlow::Wait` to conserve CPU.
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // Apply coalesced resize before blocking.
        if let Some(new_size) = self.pending_resize.take()
            && let (Some(renderer), Some(terminal)) =
                (self.renderer.as_mut(), self.terminal.as_mut())
        {
            terminal.resize_with_renderer(renderer, new_size);
            self.pty.resize(new_size);
        }

        let should_blink = self
            .terminal
            .as_ref()
            .is_some_and(|t| t.screen().cursor_blink());

        if should_blink {
            let next = self.cursor_blink.check_blink(self.window.as_deref());
            event_loop.set_control_flow(ControlFlow::WaitUntil(next));
        } else {
            event_loop.set_control_flow(ControlFlow::Wait);
        }
    }

    /// Dispatches window-level events: resize, redraw, close, keyboard input.
    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(window) = self.window.as_ref() else {
            return;
        };
        if window.id() != window_id {
            return;
        }

        match event {
            WindowEvent::CloseRequested => {
                tracing::info!("close requested");
                event_loop.exit();
            }

            // Resize: update surface config immediately, but defer terminal
            // grid + PTY resize to `about_to_wait` to coalesce bounce.
            WindowEvent::Resized(size) => {
                tracing::trace!(width = size.width, height = size.height, "window resized");
                let new_size = self
                    .renderer
                    .as_mut()
                    .and_then(|renderer| renderer.resize(size.width, size.height));
                if let Some(new_size) = new_size {
                    self.pending_resize = Some(new_size);
                }
                window.request_redraw();
            }

            // Redraw: sync cursor blink visibility, upload cursor vertices,
            // commit the blink state, then draw a frame and present it.
            WindowEvent::RedrawRequested => {
                tracing::trace!("redraw requested");
                if let (Some(renderer), Some(terminal)) =
                    (self.renderer.as_mut(), self.terminal.as_ref())
                {
                    let screen = terminal.screen();
                    let visible = if screen.cursor_blink() {
                        self.cursor_blink.visible()
                    } else {
                        true // steady cursor: always visible
                    };
                    renderer.set_cursor_visible(visible, screen);
                    renderer.prepare_cursor(screen);
                }
                self.cursor_blink.commit_frame();
                if let Some(renderer) = self.renderer.as_mut() {
                    renderer.render();
                }
            }
            // Track modifier state for Ctrl+letter → control-character mapping.
            WindowEvent::ModifiersChanged(modifiers) => {
                self.modifiers = modifiers.state();
            }
            // Mouse wheel → scroll viewport through history.
            WindowEvent::MouseWheel { delta, .. } => {
                let Some((terminal, renderer, window)) = self
                    .terminal
                    .as_mut()
                    .zip(self.renderer.as_mut())
                    .zip(self.window.as_ref())
                    .map(|((t, r), w)| (t, r, w))
                else {
                    return;
                };

                if terminal.is_alt_screen() {
                    return;
                }
                let lines = match delta {
                    MouseScrollDelta::LineDelta(_, y) => (y * 3.0) as isize,
                    MouseScrollDelta::PixelDelta(pos) => (pos.y / 20.0) as isize,
                };
                if lines > 0 {
                    terminal.scroll_viewport_up(lines as usize);
                } else if lines < 0 {
                    terminal.scroll_viewport_down((-lines) as usize);
                }

                renderer.prepare_layers(terminal.screen());
                terminal.clear_screen_dirty();
                window.request_redraw();
            }
            // Keyboard press → forward the key event to the PTY stdin pipe.
            WindowEvent::KeyboardInput {
                device_id: _,
                event,
                is_synthetic: _,
            } if event.state == ElementState::Pressed => {
                // Snap viewport back to live on any key press.
                if let Some(terminal) = self.terminal.as_mut() {
                    terminal.scroll_viewport_to_bottom();
                }

                let Some(bytes) =
                    keyboard_input_bytes(&event.logical_key, event.text.as_deref(), self.modifiers)
                else {
                    return;
                };
                self.pty.write(&bytes);
            }
            _ => {}
        }
    }
}

// ── App (own methods) ─────────────────────────────────────────────────────

impl App {
    /// Creates the application shell with no initial window, renderer, or PTY.
    /// These are lazily initialised on the first `resumed` call.
    pub(crate) fn new(event_proxy: EventLoopProxy<AppEvent>) -> Self {
        Self {
            window: None,
            renderer: None,
            terminal: None,
            pty: Pty::new(event_proxy),
            cursor_blink: CursorBlink::new(),
            pending_resize: None,
            modifiers: ModifiersState::default(),
        }
    }

    /// Creates the main window and renderer; keeps existing state on
    /// repeated resumes (e.g. after suspend/resume on mobile platforms).
    fn try_resume(&mut self, event_loop: &ActiveEventLoop) -> std::result::Result<(), AppError> {
        if self.window.is_some() {
            return Ok(());
        }

        tracing::info!("creating window");
        let window =
            Arc::new(event_loop.create_window(Window::default_attributes().with_title("Harbor"))?);

        // Bootstrap with a 1×1 terminal so the glyph atlas has a screen to
        // build from.  The renderer computes the real grid size from font
        // metrics and surface dimensions.
        let mut terminal = Terminal::new(1, 1);
        let renderer = pollster::block_on(Renderer::new(window.clone(), terminal.screen()))
            .map_err(AppError::Renderer)?;
        let size = renderer.terminal_size();
        terminal.resize(size.rows, size.cols);
        tracing::info!(rows = size.rows, cols = size.cols, "terminal initialized");

        self.renderer = Some(renderer);
        self.terminal = Some(terminal);
        self.pty.start(size).map_err(AppError::Pty)?;
        self.window = Some(window.clone());
        window.request_redraw();
        Ok(())
    }
}
// ── Key mapping ───────────────────────────────────────────────────────────

/// Maps a logical key + optional text + modifier state to the byte sequence
/// to write to the PTY.  Named control/navigation keys are dispatched by
/// `logical_key` first.  When Ctrl is held and the key is a single ASCII
/// letter (a–z / A–Z), the corresponding control character (0x01–0x1A) is
/// emitted regardless of what winit places in `text`.
fn keyboard_input_bytes(
    logical_key: &Key,
    text: Option<&str>,
    modifiers: ModifiersState,
) -> Option<Vec<u8>> {
    // Ctrl+letter → control character (0x01–0x1A).
    if modifiers.control_key()
        && let Key::Character(ch) = logical_key
        && let Some(ctrl_byte) = ctrl_letter_to_byte(ch)
    {
        return Some(vec![ctrl_byte]);
    }

    // If it's some other character with Ctrl held, fall through —
    // winit may have placed a control character in `text` already.
    match logical_key {
        Key::Named(NamedKey::Enter) => Some(b"\r".to_vec()),
        Key::Named(NamedKey::Backspace) => Some(b"\x7f".to_vec()),
        Key::Named(NamedKey::Tab) => Some(b"\t".to_vec()),
        Key::Named(NamedKey::Escape) => Some(b"\x1b".to_vec()),
        // Arrow keys → standard VT100 escape sequences.
        Key::Named(NamedKey::ArrowUp) => Some(b"\x1b[A".to_vec()),
        Key::Named(NamedKey::ArrowDown) => Some(b"\x1b[B".to_vec()),
        Key::Named(NamedKey::ArrowRight) => Some(b"\x1b[C".to_vec()),
        Key::Named(NamedKey::ArrowLeft) => Some(b"\x1b[D".to_vec()),
        // For everything else, send the UTF-8 text if present.
        _ => {
            let t = text?;
            if t.is_empty() {
                None
            } else {
                Some(t.as_bytes().to_vec())
            }
        }
    }
}

/// Converts a single-character `SmolStr` to its control-character byte
/// (`letter & 0x1F`).  Returns `None` for multi-codepoint strings or
/// non-ASCII letters.
fn ctrl_letter_to_byte(ch: &str) -> Option<u8> {
    let mut chars = ch.chars();
    let c = chars.next()?;
    if chars.next().is_some() {
        return None; // more than one codepoint — not a simple letter
    }
    match c {
        'a'..='z' => Some((c as u8) - b'a' + 1),
        'A'..='Z' => Some((c as u8) - b'A' + 1),
        _ => None,
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use winit::keyboard::{Key, ModifiersState, NamedKey};

    fn k(name: NamedKey) -> Key {
        Key::Named(name)
    }

    fn mods() -> ModifiersState {
        ModifiersState::default()
    }

    fn ctrl() -> ModifiersState {
        ModifiersState::CONTROL
    }

    #[test]
    fn backspace_with_unexpected_text_still_sends_del() {
        assert_eq!(
            keyboard_input_bytes(&k(NamedKey::Backspace), Some("\x17"), mods()),
            Some(b"\x7f".to_vec())
        );
    }

    #[test]
    fn backspace_with_no_text_sends_del() {
        assert_eq!(
            keyboard_input_bytes(&k(NamedKey::Backspace), None, mods()),
            Some(b"\x7f".to_vec())
        );
    }

    #[test]
    fn backspace_with_empty_text_sends_del() {
        assert_eq!(
            keyboard_input_bytes(&k(NamedKey::Backspace), Some(""), mods()),
            Some(b"\x7f".to_vec())
        );
    }

    #[test]
    fn enter_sends_cr() {
        assert_eq!(
            keyboard_input_bytes(&k(NamedKey::Enter), None, mods()),
            Some(b"\r".to_vec())
        );
    }

    #[test]
    fn escape_sends_esc() {
        assert_eq!(
            keyboard_input_bytes(&k(NamedKey::Escape), None, mods()),
            Some(b"\x1b".to_vec())
        );
    }

    #[test]
    fn arrow_up() {
        assert_eq!(
            keyboard_input_bytes(&k(NamedKey::ArrowUp), None, mods()),
            Some(b"\x1b[A".to_vec())
        );
    }

    #[test]
    fn printable_character() {
        assert_eq!(
            keyboard_input_bytes(&Key::Character("a".into()), Some("a"), mods()),
            Some(b"a".to_vec())
        );
    }

    #[test]
    fn unrecognized_named_key_no_text_ignored() {
        assert_eq!(keyboard_input_bytes(&k(NamedKey::F1), None, mods()), None);
    }

    #[test]
    fn empty_text_ignored() {
        assert_eq!(
            keyboard_input_bytes(&Key::Character("".into()), Some(""), mods()),
            None
        );
    }

    // ── Ctrl+letter → control character ───────────────────────────────

    #[test]
    fn ctrl_c_sends_etx_with_text() {
        // Ctrl held + 'c' → 0x03, even if winit puts plain "c" in text.
        assert_eq!(
            keyboard_input_bytes(&Key::Character("c".into()), Some("c"), ctrl()),
            Some(b"\x03".to_vec())
        );
    }

    #[test]
    fn ctrl_c_sends_etx_without_text() {
        assert_eq!(
            keyboard_input_bytes(&Key::Character("c".into()), None, ctrl()),
            Some(b"\x03".to_vec())
        );
    }

    #[test]
    fn ctrl_d_sends_eot() {
        assert_eq!(
            keyboard_input_bytes(&Key::Character("d".into()), Some("d"), ctrl()),
            Some(b"\x04".to_vec())
        );
    }

    #[test]
    fn ctrl_shift_c_still_sends_etx() {
        // Ctrl+Shift+C = Ctrl+C → 0x03.
        assert_eq!(
            keyboard_input_bytes(&Key::Character("C".into()), Some("C"), ctrl()),
            Some(b"\x03".to_vec())
        );
    }

    #[test]
    fn c_without_ctrl_is_plain_text() {
        // 'c' without Ctrl held → normal text.
        assert_eq!(
            keyboard_input_bytes(&Key::Character("c".into()), Some("c"), mods()),
            Some(b"c".to_vec())
        );
    }

    #[test]
    fn c_without_text_or_ctrl_is_none() {
        assert_eq!(
            keyboard_input_bytes(&Key::Character("c".into()), None, mods()),
            None
        );
    }

    #[test]
    fn ctrl_non_letter_falls_through_to_text() {
        // Ctrl+1 is not a letter — should use winit's text if provided.
        assert_eq!(
            keyboard_input_bytes(&Key::Character("1".into()), Some("1"), ctrl()),
            Some(b"1".to_vec())
        );
    }
}

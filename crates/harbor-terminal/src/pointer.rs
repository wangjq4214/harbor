//! Terminal-owned pointer interaction state.
//!
//! This module deliberately contains no window or GPU handles. It arbitrates
//! text selection and scrollbar gestures using terminal snapshots and the
//! render viewport supplied by the terminal host.

use crate::render::{RenderViewport, ScrollbarHit, hit_test, offset_for_thumb};
use crate::{AutoScroll, GenPos, Screen, SelectionBounds, SelectionModel, SelectionOutcome};
use crate::{
    TerminalEventOutcome, TerminalPointerButton, TerminalPointerEvent, TerminalPointerPhase,
};
use std::time::Instant;

#[derive(Clone, Copy, Debug, PartialEq)]
enum ActivePointer {
    Selection { pointer_id: u64 },
    Scrollbar { pointer_id: u64, grab_offset: f32 },
}

pub struct PointerInteraction {
    selection: SelectionModel,
    active: Option<ActivePointer>,
    viewport: Option<RenderViewport>,
    input_scale: f32,
    mouse_buttons: u8,
    vt_capture: Option<u64>,
}

impl Default for PointerInteraction {
    fn default() -> Self {
        Self::new()
    }
}

impl PointerInteraction {
    pub fn new() -> Self {
        Self {
            selection: SelectionModel::new(),
            active: None,
            viewport: None,
            input_scale: 1.0,
            mouse_buttons: 0,
            vt_capture: None,
        }
    }

    pub fn set_viewport(&mut self, viewport: RenderViewport) {
        self.viewport = Some(viewport);
    }

    pub fn set_input_scale(&mut self, scale_factor: f32) {
        self.input_scale = scale_factor.max(0.001);
    }

    pub fn has_viewport(&self) -> bool {
        self.viewport.is_some()
    }

    pub fn has_active_pointer(&self) -> bool {
        self.active.is_some() || self.vt_capture.is_some()
    }

    pub fn begin_vt_capture(&mut self, pointer_id: u64) {
        self.vt_capture = Some(pointer_id);
    }

    pub fn end_vt_capture(&mut self, pointer_id: u64) -> bool {
        if self.vt_capture == Some(pointer_id) {
            self.vt_capture = None;
            true
        } else {
            false
        }
    }

    pub fn report_position(
        &self,
        position: (f32, f32),
        snapshot: &harbor_types::TerminalSnapshot,
    ) -> Option<(f32, f32)> {
        let viewport = self.viewport?;
        let position = self.physical_position(position);
        let x = (position.0 - viewport.allocation_origin.0 - viewport.padding).max(0.0);
        let y = (position.1 - viewport.allocation_origin.1 - viewport.padding).max(0.0);
        let col = ((x / viewport.cell_width).floor() as usize).min(snapshot.cols.saturating_sub(1));
        let row =
            ((y / viewport.line_height).floor() as usize).min(snapshot.rows.saturating_sub(1));
        Some((col as f32, row as f32))
    }

    pub fn clear(&mut self) {
        self.selection.clear();
        self.active = None;
        self.mouse_buttons = 0;
    }

    pub fn prepare_mouse_event(&mut self, mut event: TerminalPointerEvent) -> TerminalPointerEvent {
        let bit = match event.button {
            TerminalPointerButton::Left => 1,
            TerminalPointerButton::Middle => 2,
            TerminalPointerButton::Right => 4,
            TerminalPointerButton::None => 0,
        };
        match event.phase {
            TerminalPointerPhase::Down => self.mouse_buttons |= bit,
            TerminalPointerPhase::Move => {
                event.button = if self.mouse_buttons & 1 != 0 {
                    TerminalPointerButton::Left
                } else if self.mouse_buttons & 2 != 0 {
                    TerminalPointerButton::Middle
                } else if self.mouse_buttons & 4 != 0 {
                    TerminalPointerButton::Right
                } else {
                    TerminalPointerButton::None
                };
            }
            TerminalPointerPhase::Up => self.mouse_buttons &= !bit,
            TerminalPointerPhase::Cancel => self.mouse_buttons = 0,
            _ => {}
        }
        event
    }

    pub fn auto_scroll_deadline(&self) -> Option<Instant> {
        self.selection.auto_scroll_deadline()
    }

    pub fn cancel(&mut self) -> TerminalEventOutcome {
        let release_pointer = self
            .active
            .take()
            .map(|active| match active {
                ActivePointer::Selection { pointer_id }
                | ActivePointer::Scrollbar { pointer_id, .. } => pointer_id,
            })
            .or_else(|| self.vt_capture.take());
        self.mouse_buttons = 0;
        let redraw = self.selection.cancel() != SelectionOutcome::None;
        TerminalEventOutcome {
            redraw,
            release_pointer,
            ..TerminalEventOutcome::default()
        }
    }

    pub fn bounds(&self) -> Option<SelectionBounds> {
        if self.selection.is_range_empty() {
            None
        } else {
            self.selection.bounds()
        }
    }

    pub fn has_non_empty_selection(&self) -> bool {
        self.selection.has_selection() && !self.selection.is_range_empty()
    }

    pub fn clear_selection(&mut self) -> bool {
        let changed = self.selection.has_selection() || self.selection.is_dragging();
        self.selection.clear();
        self.active = None;
        changed
    }

    pub fn clear_selection_outcome(&mut self) -> TerminalEventOutcome {
        let release_pointer = self.active.take().map(|active| match active {
            ActivePointer::Selection { pointer_id }
            | ActivePointer::Scrollbar { pointer_id, .. } => pointer_id,
        });
        self.mouse_buttons = 0;
        let redraw = self.clear_selection();
        TerminalEventOutcome {
            redraw,
            release_pointer,
            ..TerminalEventOutcome::default()
        }
    }

    pub fn on_key_press_outcome(&mut self) -> TerminalEventOutcome {
        let release_pointer = self.active.take().map(|active| match active {
            ActivePointer::Selection { pointer_id }
            | ActivePointer::Scrollbar { pointer_id, .. } => pointer_id,
        });
        self.mouse_buttons = 0;
        let redraw = self.selection.on_key_press();
        TerminalEventOutcome {
            redraw,
            release_pointer,
            ..TerminalEventOutcome::default()
        }
    }

    pub fn on_key_press(&mut self) -> bool {
        let changed = self.selection.on_key_press();
        if changed {
            self.active = None;
        }
        changed
    }

    pub fn handle_pointer(
        &mut self,
        screen: &mut Screen,
        event: TerminalPointerEvent,
        now: Instant,
    ) -> TerminalEventOutcome {
        let Some(viewport) = self.viewport else {
            return TerminalEventOutcome::default();
        };
        let snapshot = screen.terminal_snapshot();
        let physical_position = self.physical_position(event.position);

        if matches!(
            event.phase,
            TerminalPointerPhase::Up | TerminalPointerPhase::Cancel
        ) && self.end_vt_capture(event.pointer_id)
        {
            return TerminalEventOutcome {
                release_pointer: Some(event.pointer_id),
                ..TerminalEventOutcome::default()
            };
        }

        match event.phase {
            TerminalPointerPhase::WheelLine { dy, .. }
            | TerminalPointerPhase::WheelPixel { dy, .. } => {
                if snapshot.is_alt {
                    return TerminalEventOutcome::default();
                }
                let lines = if matches!(event.phase, TerminalPointerPhase::WheelLine { .. }) {
                    (dy * 3.0) as isize
                } else {
                    (dy / 20.0) as isize
                };
                if lines > 0 {
                    screen.scroll_up(lines as usize);
                } else if lines < 0 {
                    screen.scroll_down(lines.unsigned_abs());
                }
                TerminalEventOutcome {
                    redraw: lines != 0,
                    ..TerminalEventOutcome::default()
                }
            }
            TerminalPointerPhase::Down if event.button == TerminalPointerButton::Left => {
                match hit_test(&snapshot, &viewport, physical_position) {
                    ScrollbarHit::Thumb { grab_offset } => {
                        self.active = Some(ActivePointer::Scrollbar {
                            pointer_id: event.pointer_id,
                            grab_offset,
                        });
                        return TerminalEventOutcome {
                            redraw: true,
                            capture_pointer: Some(event.pointer_id),
                            ..TerminalEventOutcome::default()
                        };
                    }
                    ScrollbarHit::TrackBefore => {
                        screen.scroll_up(snapshot.rows);
                        return TerminalEventOutcome {
                            redraw: true,
                            ..TerminalEventOutcome::default()
                        };
                    }
                    ScrollbarHit::TrackAfter => {
                        screen.scroll_down(snapshot.rows);
                        return TerminalEventOutcome {
                            redraw: true,
                            ..TerminalEventOutcome::default()
                        };
                    }
                    ScrollbarHit::None => {}
                }

                let cell = self.pixel_to_cell(event.position, &snapshot, &viewport);
                let was_visible =
                    self.selection.has_selection() && !self.selection.is_range_empty();
                let outcome = self.selection.press(cell, now, &snapshot);
                let is_visible = !self.selection.is_range_empty();
                self.active = Some(ActivePointer::Selection {
                    pointer_id: event.pointer_id,
                });
                TerminalEventOutcome {
                    // A first press creates only an empty anchor and must not
                    // paint a cell; a prior visible selection still needs to
                    // be cleared immediately.
                    redraw: outcome != SelectionOutcome::None
                        && (was_visible != is_visible || is_visible),
                    capture_pointer: Some(event.pointer_id),
                    ..TerminalEventOutcome::default()
                }
            }
            TerminalPointerPhase::Move => {
                let Some(active) = self.active else {
                    return TerminalEventOutcome::default();
                };
                match active {
                    ActivePointer::Selection { pointer_id } if pointer_id == event.pointer_id => {
                        let cell = self.pixel_to_cell(event.position, &snapshot, &viewport);
                        let changed = self.selection.drag_to(cell, &snapshot);
                        let auto_scrolled = self.tick(screen, now);
                        TerminalEventOutcome {
                            redraw: changed
                                || auto_scrolled
                                || self.selection.auto_scroll_direction().is_some(),
                            ..TerminalEventOutcome::default()
                        }
                    }
                    ActivePointer::Scrollbar {
                        pointer_id,
                        grab_offset,
                    } if pointer_id == event.pointer_id => {
                        let Some(offset) = offset_for_thumb(
                            &snapshot,
                            &viewport,
                            physical_position.1,
                            grab_offset,
                        ) else {
                            return TerminalEventOutcome::default();
                        };
                        let changed = offset != snapshot.view_offset;
                        if offset > snapshot.view_offset {
                            screen.scroll_up(offset - snapshot.view_offset);
                        } else {
                            screen.scroll_down(snapshot.view_offset - offset);
                        }
                        TerminalEventOutcome {
                            redraw: changed,
                            ..TerminalEventOutcome::default()
                        }
                    }
                    _ => TerminalEventOutcome::default(),
                }
            }
            TerminalPointerPhase::Up | TerminalPointerPhase::Cancel => {
                let Some(active) = self.active.take() else {
                    return TerminalEventOutcome::default();
                };
                let matches = match active {
                    ActivePointer::Selection { pointer_id }
                    | ActivePointer::Scrollbar { pointer_id, .. } => pointer_id == event.pointer_id,
                };
                if !matches {
                    self.active = Some(active);
                    return TerminalEventOutcome::default();
                }
                let redraw = match active {
                    ActivePointer::Selection { .. } => {
                        if matches!(event.phase, TerminalPointerPhase::Cancel) {
                            self.selection.cancel() != SelectionOutcome::None
                        } else {
                            self.selection.release() != SelectionOutcome::None
                        }
                    }
                    ActivePointer::Scrollbar { .. } => true,
                };
                TerminalEventOutcome {
                    redraw,
                    release_pointer: Some(event.pointer_id),
                    ..TerminalEventOutcome::default()
                }
            }
            _ => TerminalEventOutcome::default(),
        }
    }

    pub fn tick(&mut self, screen: &mut Screen, now: Instant) -> bool {
        let snapshot = screen.terminal_snapshot();
        let Some((direction, cursor)) = self.selection.compute_auto_scroll_cursor(now, &snapshot)
        else {
            return false;
        };
        match direction {
            AutoScroll::Up => screen.scroll_up(1),
            AutoScroll::Down => screen.scroll_down(1),
        }
        let snapshot = screen.terminal_snapshot();
        let _ = self.selection.drag_to(GenPos::from(cursor), &snapshot);
        true
    }

    fn physical_position(&self, position: (f32, f32)) -> (f32, f32) {
        (position.0 * self.input_scale, position.1 * self.input_scale)
    }

    fn pixel_to_cell(
        &self,
        position: (f32, f32),
        snapshot: &harbor_types::TerminalSnapshot,
        viewport: &RenderViewport,
    ) -> GenPos {
        let position = self.physical_position(position);
        let x = (position.0 - viewport.allocation_origin.0 - viewport.padding).max(0.0);
        let y = (position.1 - viewport.allocation_origin.1 - viewport.padding).max(0.0);
        let row = (y / viewport.line_height).floor() as usize;
        let col = (x / viewport.cell_width).floor() as usize;
        let row = row.min(snapshot.rows.saturating_sub(1));
        let col = col.min(snapshot.cols.saturating_sub(1));
        let view_start = snapshot.history_start
            + snapshot.scroll_count.saturating_sub(snapshot.view_offset) as u64;
        GenPos::new(view_start + row as u64, col)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::RenderViewport;

    fn viewport() -> RenderViewport {
        RenderViewport::with_padding(10.0, 20.0, 0.0)
    }

    fn pointer_event(
        position: (f32, f32),
        phase: TerminalPointerPhase,
        pointer_id: u64,
    ) -> TerminalPointerEvent {
        TerminalPointerEvent::new(position, phase, TerminalPointerButton::Left, pointer_id)
    }

    #[test]
    fn should_release_reported_pointer_capture_on_focus_cancel() {
        let mut pointer = PointerInteraction::new();
        pointer.begin_vt_capture(42);
        assert!(pointer.has_active_pointer());

        let outcome = pointer.cancel();

        assert_eq!(outcome.release_pointer, Some(42));
        assert!(!pointer.has_active_pointer());
    }

    #[test]
    fn should_select_cells_when_a_local_pointer_drag_moves_between_cells() {
        // Arrange
        let mut screen = Screen::new(2, 10);
        for ch in "abcdefghij".chars() {
            screen.write_char(ch);
        }
        let mut pointer = PointerInteraction::new();
        pointer.set_viewport(viewport());
        let now = Instant::now();

        // Act
        let pressed = pointer.handle_pointer(
            &mut screen,
            pointer_event((11.0, 1.0), TerminalPointerPhase::Down, 7),
            now,
        );
        assert!(pointer.bounds().is_none());
        let moved = pointer.handle_pointer(
            &mut screen,
            pointer_event((61.0, 1.0), TerminalPointerPhase::Move, 7),
            now,
        );
        let released = pointer.handle_pointer(
            &mut screen,
            pointer_event((61.0, 1.0), TerminalPointerPhase::Up, 7),
            now,
        );

        // Assert
        assert_eq!(pressed.capture_pointer, Some(7));
        assert!(moved.redraw);
        assert_eq!(released.release_pointer, Some(7));
        assert_eq!(
            pointer.bounds(),
            Some(SelectionBounds {
                start_row: 0,
                start_col: 1,
                end_row: 0,
                end_col: 6,
            })
        );
    }

    #[test]
    fn should_select_a_word_when_the_same_cell_is_double_clicked_locally() {
        // Arrange
        let mut screen = Screen::new(2, 10);
        for ch in "abcdefghij".chars() {
            screen.write_char(ch);
        }
        let mut pointer = PointerInteraction::new();
        pointer.set_viewport(viewport());
        let now = Instant::now();
        let event = pointer_event((31.0, 1.0), TerminalPointerPhase::Down, 1);
        let release = pointer_event((31.0, 1.0), TerminalPointerPhase::Up, 1);
        pointer.handle_pointer(&mut screen, event, now);
        pointer.handle_pointer(&mut screen, release, now);

        // Act
        pointer.handle_pointer(&mut screen, event, now);

        // Assert
        assert_eq!(
            pointer.bounds(),
            Some(SelectionBounds {
                start_row: 0,
                start_col: 0,
                end_row: 0,
                end_col: 9,
            })
        );
        assert!(pointer.has_non_empty_selection());
    }

    #[test]
    fn should_select_the_full_line_when_the_same_cell_is_triple_clicked_locally() {
        // Arrange
        let mut screen = Screen::new(2, 10);
        for ch in "abcdefghij".chars() {
            screen.write_char(ch);
        }
        let mut pointer = PointerInteraction::new();
        pointer.set_viewport(viewport());
        let now = Instant::now();
        let event = pointer_event((31.0, 1.0), TerminalPointerPhase::Down, 1);
        let release = pointer_event((31.0, 1.0), TerminalPointerPhase::Up, 1);
        for _ in 0..2 {
            pointer.handle_pointer(&mut screen, event, now);
            pointer.handle_pointer(&mut screen, release, now);
        }

        // Act
        pointer.handle_pointer(&mut screen, event, now);

        // Assert
        assert_eq!(
            pointer.bounds(),
            Some(SelectionBounds {
                start_row: 0,
                start_col: 0,
                end_row: 0,
                end_col: 9,
            })
        );
    }
}

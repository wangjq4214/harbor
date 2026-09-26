//! Terminal-owned pointer interaction state.
//!
//! This module deliberately contains no window or GPU handles. It arbitrates
//! text selection and scrollbar gestures using terminal snapshots and the
//! render viewport supplied by the terminal host.

use crate::content_anchor::{AnchorMutationBatch, ContentProjection};
use crate::model::AltScreenAction;
use crate::render::{RenderViewport, ScrollbarHit, hit_test, offset_for_thumb};
use crate::screen::PreparedScreenResize;
use crate::{AutoScroll, GenPos, Screen, SelectionBounds, SelectionModel, SelectionOutcome};
use crate::{
    TerminalEventOutcome, TerminalPointerButton, TerminalPointerEvent, TerminalPointerPhase,
};
use std::time::Instant;

#[derive(Clone, Debug, PartialEq)]
struct HyperlinkPress {
    cell: GenPos,
    hyperlink: crate::model::HyperlinkId,
    pressed_cell: crate::Cell,
}

#[derive(Clone, Debug, PartialEq)]
enum ActivePointer {
    Selection {
        pointer_id: u64,
        hyperlink: Option<HyperlinkPress>,
    },
    Scrollbar {
        pointer_id: u64,
        grab_offset: f32,
    },
}

impl ActivePointer {
    fn pointer_id(self) -> u64 {
        match self {
            Self::Selection { pointer_id, .. } | Self::Scrollbar { pointer_id, .. } => pointer_id,
        }
    }
}
pub(crate) struct PreparedPointerResize {
    selection: SelectionModel,
    saved_primary_selection: Option<SelectionModel>,
    parked_alt_selection: Option<SelectionModel>,
    active_invalidated: bool,
}

pub struct PointerInteraction {
    selection: SelectionModel,
    saved_primary_selection: Option<SelectionModel>,
    parked_alt_selection: Option<SelectionModel>,
    active: Option<ActivePointer>,
    viewport: Option<RenderViewport>,
    input_scale: f32,
    mouse_buttons: u8,
    vt_capture: Option<u64>,
    pending_release: Option<u64>,
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
            saved_primary_selection: None,
            parked_alt_selection: None,
            active: None,
            viewport: None,
            input_scale: 1.0,
            mouse_buttons: 0,
            vt_capture: None,
            pending_release: None,
        }
    }

    pub(crate) fn prepare_resize(&self, screen: &PreparedScreenResize) -> PreparedPointerResize {
        let (selection, active_invalidated) = self
            .selection
            .prepared_against(|anchor| screen.project_active_selection(anchor));
        let saved_primary_selection = self.saved_primary_selection.as_ref().and_then(|selection| {
            screen.has_saved_primary().then(|| {
                selection
                    .prepared_against(|anchor| screen.project_saved_primary_selection(anchor))
                    .0
            })
        });
        let parked_alt_selection = self.parked_alt_selection.as_ref().and_then(|selection| {
            screen.has_parked_alt().then(|| {
                selection
                    .prepared_against(|anchor| screen.project_parked_alt_selection(anchor))
                    .0
            })
        });
        PreparedPointerResize {
            selection,
            saved_primary_selection,
            parked_alt_selection,
            active_invalidated,
        }
    }

    pub(crate) fn commit_resize(&mut self, prepared: PreparedPointerResize) {
        self.selection = prepared.selection;
        self.saved_primary_selection = prepared.saved_primary_selection;
        self.parked_alt_selection = prepared.parked_alt_selection;
        if prepared.active_invalidated {
            self.queue_local_release();
            self.mouse_buttons = 0;
        }
    }

    pub(crate) fn apply_alt_transition(&mut self, screen: &mut Screen, action: AltScreenAction) {
        match action {
            AltScreenAction::Enter { clear } if !screen.is_alt() => {
                self.cancel_active_for_buffer_transition();
                let alternate = if clear {
                    self.parked_alt_selection = None;
                    SelectionModel::new()
                } else if screen.has_parked_alt() {
                    self.parked_alt_selection.take().unwrap_or_default()
                } else {
                    self.parked_alt_selection = None;
                    SelectionModel::new()
                };
                let primary = std::mem::replace(&mut self.selection, alternate);
                self.saved_primary_selection = Some(primary);
                screen.enter_alt(clear);
            }
            AltScreenAction::Exit if screen.is_alt() => {
                self.cancel_active_for_buffer_transition();
                let primary = self.saved_primary_selection.take().unwrap_or_default();
                let alternate = std::mem::replace(&mut self.selection, primary);
                self.parked_alt_selection = Some(alternate);
                screen.exit_alt();
            }
            _ => {}
        }
    }

    fn cancel_active_for_buffer_transition(&mut self) {
        let _ = self.selection.cancel();
        self.queue_local_release();
        self.mouse_buttons = 0;
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
        self.active.is_some() || self.vt_capture.is_some() || self.pending_release.is_some()
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

    /// A local capture may have been queued for release before VT reporting turned on.
    pub(crate) fn release_vt_or_pending(&mut self, pointer_id: u64) -> bool {
        let captured = self.end_vt_capture(pointer_id);
        if self.pending_release == Some(pointer_id) {
            self.pending_release = None;
            true
        } else {
            captured
        }
    }

    pub fn report_position(
        &self,
        position: (f32, f32),
        snapshot: &crate::model::TerminalSnapshot,
    ) -> Option<(f32, f32)> {
        let viewport = self.viewport?;
        let (row, col) = self.grid_cell(position, snapshot, &viewport);
        Some((col as f32, row as f32))
    }

    pub fn clear(&mut self) {
        self.queue_local_release();
        self.selection.clear();
        self.saved_primary_selection = None;
        self.parked_alt_selection = None;
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
        let release_pointer = self.consume_release_sources();
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
    pub(crate) fn has_selection_state(&self) -> bool {
        self.selection.has_selection()
    }

    pub fn clear_selection_outcome(&mut self) -> TerminalEventOutcome {
        let redraw = self.selection.has_selection() || self.selection.is_dragging();
        self.selection.clear();
        self.interrupt_outcome(redraw)
    }

    pub fn on_key_press_outcome(&mut self) -> TerminalEventOutcome {
        let redraw = self.selection.on_key_press();
        self.interrupt_outcome(redraw)
    }

    /// Interrupts only local selection/scrollbar capture when VT mouse reporting takes over.
    pub fn cancel_local_pointer(&mut self) -> TerminalEventOutcome {
        self.local_interruption()
    }

    fn queue_local_release(&mut self) {
        self.pending_release = self
            .active
            .take()
            .map(ActivePointer::pointer_id)
            .or(self.pending_release);
    }

    fn consume_release_sources(&mut self) -> Option<u64> {
        self.active
            .take()
            .map(ActivePointer::pointer_id)
            .or_else(|| self.vt_capture.take())
            .or_else(|| self.pending_release.take())
    }

    fn local_interruption(&mut self) -> TerminalEventOutcome {
        let release_pointer = self.active.take().map(ActivePointer::pointer_id);
        let redraw = self.selection.cancel() != SelectionOutcome::None;
        TerminalEventOutcome {
            redraw,
            release_pointer,
            ..TerminalEventOutcome::default()
        }
    }

    /// Releases the active pointer and clears button state after a selection interrupt.
    fn interrupt_outcome(&mut self, redraw: bool) -> TerminalEventOutcome {
        let release_pointer = self
            .active
            .take()
            .map(ActivePointer::pointer_id)
            .or_else(|| self.pending_release.take());
        self.mouse_buttons = 0;
        TerminalEventOutcome {
            redraw,
            release_pointer,
            ..TerminalEventOutcome::default()
        }
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
        let physical_position = self.physical_position(event.position);
        if matches!(
            event.phase,
            TerminalPointerPhase::Up | TerminalPointerPhase::Cancel
        ) && self.pending_release == Some(event.pointer_id)
        {
            self.pending_release = None;
            return TerminalEventOutcome {
                release_pointer: Some(event.pointer_id),
                ..TerminalEventOutcome::default()
            };
        }

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

        if let TerminalPointerPhase::WheelLine { dy, .. }
        | TerminalPointerPhase::WheelPixel { dy, .. } = event.phase
        {
            let interrupted = self.local_interruption();
            let is_pixel = matches!(event.phase, TerminalPointerPhase::WheelPixel { .. });
            let lines = screen.scroll_wheel(dy, is_pixel);
            return TerminalEventOutcome {
                redraw: lines != 0 || interrupted.redraw,
                release_pointer: interrupted.release_pointer,
                ..TerminalEventOutcome::default()
            };
        }

        let snapshot = screen.terminal_snapshot();
        match event.phase {
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
                let hyperlink = screen
                    .cell_at_generation(cell.generation, cell.col)
                    .and_then(|pressed_cell| {
                        pressed_cell.hyperlink.map(|hyperlink| HyperlinkPress {
                            cell,
                            hyperlink,
                            pressed_cell: pressed_cell.clone(),
                        })
                    });
                let outcome = self.selection.press(cell, now, &snapshot);
                let _ = self.commit_selection(screen);
                let is_visible = !self.selection.is_range_empty();
                self.active = Some(ActivePointer::Selection {
                    pointer_id: event.pointer_id,
                    hyperlink: (!is_visible).then_some(hyperlink).flatten(),
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
                let Some(active) = self.active.clone() else {
                    return TerminalEventOutcome::default();
                };
                match active {
                    ActivePointer::Selection { pointer_id, .. }
                        if pointer_id == event.pointer_id =>
                    {
                        let cell = self.pixel_to_cell(event.position, &snapshot, &viewport);
                        let changed = self.selection.drag_to(cell, &snapshot);
                        let _ = self.commit_selection(screen);
                        let auto_scrolled = self.tick(screen, now);
                        let auto_scrolling = self.selection.auto_scroll_direction().is_some();
                        if (changed || auto_scrolled || auto_scrolling)
                            && let Some(ActivePointer::Selection { hyperlink, .. }) =
                                self.active.as_mut()
                        {
                            *hyperlink = None;
                        }
                        TerminalEventOutcome {
                            redraw: changed || auto_scrolled || auto_scrolling,
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
                if active.clone().pointer_id() != event.pointer_id {
                    self.active = Some(active);
                    return TerminalEventOutcome::default();
                }
                let (redraw, hyperlink_activation) = match active {
                    ActivePointer::Selection { hyperlink, .. } => {
                        let activation = if matches!(event.phase, TerminalPointerPhase::Cancel)
                            || event.button != TerminalPointerButton::Left
                        {
                            None
                        } else {
                            let release = self.pixel_to_cell(event.position, &snapshot, &viewport);
                            hyperlink.and_then(|press| {
                                let original_cell = screen
                                    .cell_at_generation(press.cell.generation, press.cell.col);
                                let original = screen
                                    .hyperlink_at_generation(press.cell.generation, press.cell.col);
                                let released =
                                    screen.hyperlink_at_generation(release.generation, release.col);
                                match (original_cell, original, released) {
                                    (
                                        Some(current_cell),
                                        Some((original_id, uri)),
                                        Some((released_id, _)),
                                    ) if *current_cell == press.pressed_cell
                                        && original_id == press.hyperlink
                                        && released_id == press.hyperlink =>
                                    {
                                        Some(uri.to_owned())
                                    }
                                    _ => None,
                                }
                            })
                        };
                        let redraw = if matches!(event.phase, TerminalPointerPhase::Cancel) {
                            self.selection.cancel() != SelectionOutcome::None
                        } else {
                            self.selection.release() != SelectionOutcome::None
                        };
                        (redraw, activation)
                    }
                    ActivePointer::Scrollbar { .. } => (true, None),
                };
                TerminalEventOutcome {
                    redraw,
                    hyperlink_activation,
                    release_pointer: Some(event.pointer_id),
                    ..TerminalEventOutcome::default()
                }
            }
            _ => TerminalEventOutcome::default(),
        }
    }
    fn commit_selection(&mut self, screen: &Screen) -> bool {
        match screen.reader().content_projection() {
            Ok(projection) => self.selection.commit_anchors(&projection),
            Err(error) => {
                tracing::error!(generation = error.generation, column = error.column, kind = ?error.kind, "selection anchor projection failed");
                self.selection.clear();
                false
            }
        }
    }

    pub(crate) fn reconcile_selection(
        &mut self,
        mutations: &AnchorMutationBatch,
        projection: &ContentProjection,
    ) -> bool {
        let had_selection = self.selection.has_selection();
        let changed = self.selection.reconcile_anchors(mutations, projection);
        if had_selection && !self.selection.has_selection() {
            self.queue_local_release();
            self.mouse_buttons = 0;
        }
        changed
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
        let _ = self.commit_selection(screen);
        true
    }

    fn physical_position(&self, position: (f32, f32)) -> (f32, f32) {
        (position.0 * self.input_scale, position.1 * self.input_scale)
    }

    /// Maps a logical pointer position to a clamped grid cell `(row, col)`.
    fn grid_cell(
        &self,
        position: (f32, f32),
        snapshot: &crate::model::TerminalSnapshot,
        viewport: &RenderViewport,
    ) -> (usize, usize) {
        let position = self.physical_position(position);
        let x = (position.0 - viewport.allocation_origin.0 - viewport.padding).max(0.0);
        let y = (position.1 - viewport.allocation_origin.1 - viewport.padding).max(0.0);
        let row =
            ((y / viewport.line_height).floor() as usize).min(snapshot.rows.saturating_sub(1));
        let col = ((x / viewport.cell_width).floor() as usize).min(snapshot.cols.saturating_sub(1));
        (row, col)
    }

    fn pixel_to_cell(
        &self,
        position: (f32, f32),
        snapshot: &crate::model::TerminalSnapshot,
        viewport: &RenderViewport,
    ) -> GenPos {
        let (row, col) = self.grid_cell(position, snapshot, viewport);
        let view_start = snapshot.history_start
            + snapshot.scroll_count.saturating_sub(snapshot.view_offset) as u64;
        GenPos::new(view_start + row as u64, col)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content_anchor::AnchorMutation;
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
    fn invalidated_active_selection_releases_capture_on_followup_up() {
        let mut screen = Screen::new(2, 10);
        for ch in "abcdefghij".chars() {
            screen.write_char(ch);
        }
        let mut pointer = PointerInteraction::new();
        pointer.set_viewport(viewport());
        let now = Instant::now();
        pointer.handle_pointer(
            &mut screen,
            pointer_event((11.0, 1.0), TerminalPointerPhase::Down, 77),
            now,
        );
        pointer.handle_pointer(
            &mut screen,
            pointer_event((61.0, 1.0), TerminalPointerPhase::Move, 77),
            now,
        );
        let mut mutations = AnchorMutationBatch::default();
        mutations.push(AnchorMutation::InvalidateAll);
        let projection = screen.content_projection().unwrap();

        assert!(pointer.reconcile_selection(&mutations, &projection));
        assert!(pointer.has_active_pointer());
        let outcome = pointer.handle_pointer(
            &mut screen,
            pointer_event((61.0, 1.0), TerminalPointerPhase::Up, 77),
            now,
        );

        assert_eq!(outcome.release_pointer, Some(77));
        assert!(!pointer.has_active_pointer());
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

    #[test]
    fn should_activate_same_hyperlink_on_plain_click_and_suppress_drag_or_replacement() {
        let mut screen = Screen::new(2, 10);
        screen.open_hyperlink("https://example.test".to_owned(), None);
        screen.write_char('a');
        screen.write_char('b');
        let mut pointer = PointerInteraction::new();
        pointer.set_viewport(viewport());
        let now = Instant::now();

        pointer.handle_pointer(
            &mut screen,
            pointer_event((1.0, 1.0), TerminalPointerPhase::Down, 7),
            now,
        );
        let clicked = pointer.handle_pointer(
            &mut screen,
            pointer_event((11.0, 1.0), TerminalPointerPhase::Up, 7),
            now,
        );
        assert_eq!(
            clicked.hyperlink_activation.as_deref(),
            Some("https://example.test")
        );

        pointer.handle_pointer(
            &mut screen,
            pointer_event((1.0, 1.0), TerminalPointerPhase::Down, 70),
            now,
        );
        let non_primary_release = pointer.handle_pointer(
            &mut screen,
            TerminalPointerEvent::new(
                (11.0, 1.0),
                TerminalPointerPhase::Up,
                TerminalPointerButton::Right,
                70,
            ),
            now,
        );
        assert!(non_primary_release.hyperlink_activation.is_none());

        pointer.handle_pointer(
            &mut screen,
            pointer_event((1.0, 1.0), TerminalPointerPhase::Down, 8),
            now,
        );
        pointer.handle_pointer(
            &mut screen,
            pointer_event((31.0, 1.0), TerminalPointerPhase::Move, 8),
            now,
        );
        let dragged = pointer.handle_pointer(
            &mut screen,
            pointer_event((11.0, 1.0), TerminalPointerPhase::Up, 8),
            now,
        );
        assert!(dragged.hyperlink_activation.is_none());

        pointer.handle_pointer(
            &mut screen,
            pointer_event((1.0, 1.0), TerminalPointerPhase::Down, 9),
            now,
        );
        screen.set_cursor_position(1, 1);
        screen.write_char('x');
        let replaced = pointer.handle_pointer(
            &mut screen,
            pointer_event((11.0, 1.0), TerminalPointerPhase::Up, 9),
            now,
        );
        assert!(replaced.hyperlink_activation.is_none());
    }

    #[test]
    fn cancel_consumes_local_vt_and_pending_releases_in_precedence_order() {
        let mut pointer = PointerInteraction::new();
        pointer.active = Some(ActivePointer::Scrollbar {
            pointer_id: 1,
            grab_offset: 0.0,
        });
        pointer.vt_capture = Some(2);
        pointer.pending_release = Some(3);
        pointer.mouse_buttons = 7;

        assert_eq!(pointer.cancel().release_pointer, Some(1));
        assert_eq!(pointer.mouse_buttons, 0);
        assert_eq!(pointer.cancel().release_pointer, Some(2));
        assert_eq!(pointer.cancel().release_pointer, Some(3));
        assert!(!pointer.has_active_pointer());
    }

    #[test]
    fn pending_release_precedes_matching_vt_release() {
        let mut screen = Screen::new(2, 10);
        let mut pointer = PointerInteraction::new();
        pointer.set_viewport(viewport());
        pointer.pending_release = Some(7);
        pointer.begin_vt_capture(7);
        let now = Instant::now();

        let pending = pointer.handle_pointer(
            &mut screen,
            pointer_event((1.0, 1.0), TerminalPointerPhase::Up, 7),
            now,
        );
        assert_eq!(pending.release_pointer, Some(7));
        assert!(pointer.has_active_pointer());

        let vt = pointer.handle_pointer(
            &mut screen,
            pointer_event((1.0, 1.0), TerminalPointerPhase::Cancel, 7),
            now,
        );
        assert_eq!(vt.release_pointer, Some(7));
        assert!(!pointer.has_active_pointer());
    }

    #[test]
    fn wheel_interrupts_only_local_capture() {
        let mut screen = Screen::new(2, 10);
        let mut pointer = PointerInteraction::new();
        pointer.set_viewport(viewport());
        let now = Instant::now();
        pointer.handle_pointer(
            &mut screen,
            pointer_event((1.0, 1.0), TerminalPointerPhase::Down, 11),
            now,
        );
        pointer.begin_vt_capture(22);

        let wheel = pointer.handle_pointer(
            &mut screen,
            TerminalPointerEvent::new(
                (1.0, 1.0),
                TerminalPointerPhase::WheelLine { dx: 0.0, dy: 1.0 },
                TerminalPointerButton::None,
                33,
            ),
            now,
        );

        assert_eq!(wheel.release_pointer, Some(11));
        assert_eq!(pointer.cancel().release_pointer, Some(22));
    }
}

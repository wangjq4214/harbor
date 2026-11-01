use super::*;

#[test]
fn alt_screen_restores_all_state_groups() {
    let mut screen = Screen::new(4, 12);

    // Configure normal screen state across all groups.
    screen.set_scroll_region(2, 4);
    screen.set_private_mode(69, true); // DECLRMM on
    screen.set_left_right_margins(3, 10);
    screen.set_private_mode(1, true); // application cursor
    screen.set_private_mode(66, true); // application keypad
    screen.set_private_mode(6, true); // origin mode
    screen.set_private_mode(7, false); // autowrap off
    screen.set_private_mode(25, false); // cursor invisible
    screen.set_cursor_style(crate::model::CursorStyleArg::SteadyBlock); // block, no blink
    screen.set_standard_mode(4, true); // insert mode
    screen.set_standard_mode(20, true); // line feed mode
    screen.set_character_protection(crate::model::CharacterProtection::Protected); // protected
    screen.set_sgr_slice(&[Some(1), Some(31), Some(42)]);
    screen.write_char('X'); // set last_char (advances cursor to 1)
    screen.active.cursor.cursor.x = 5;
    screen.active.cursor.cursor.y = 2;
    screen.set_tab_stop();
    screen.save_cursor();
    screen.designate_g0(b'0');
    screen.designate_g1(b'B');
    screen.set_active_charset(1);
    // Enter alt — all groups should be saved.
    screen.enter_alt(true);

    // Mutate every group in alt.
    screen.active.cursor.cursor.x = 0;
    screen.active.cursor.cursor.y = 0;
    screen.set_private_mode(69, false);
    screen.set_private_mode(1, false);
    screen.set_private_mode(66, false);
    screen.set_private_mode(6, false);
    screen.set_private_mode(7, true); // autowrap on
    screen.set_private_mode(25, true); // cursor visible
    screen.set_cursor_style(crate::model::CursorStyleArg::BlinkingBar); // bar, blink
    screen.set_standard_mode(4, false); // replace mode
    screen.set_standard_mode(20, false); // normal line feed
    screen.set_character_protection(crate::model::CharacterProtection::Unprotected); // not protected
    screen.set_sgr_slice(&[Some(0)]);
    screen.set_scroll_region(0, 0);
    screen.clear_tab_stops(3);
    screen.designate_g0(b'B');
    screen.designate_g1(b'0');
    screen.set_active_charset(0);

    // Exit alt.
    screen.exit_alt();

    // Assert all state groups restored.
    // CursorState
    assert_eq!(screen.active.cursor.cursor.x, 5, "cursor.x restored");
    assert_eq!(screen.active.cursor.cursor.y, 2, "cursor.y restored");
    assert_eq!(
        screen.active.cursor.cursor.shape,
        CursorShape::Block,
        "cursor shape restored"
    );
    assert!(!screen.active.cursor.cursor.blink, "cursor blink restored");
    assert!(
        !screen.active.cursor.cursor.visible,
        "cursor visible restored"
    );
    // Pen
    assert!(
        screen.active.pen_state.pen.attrs.contains(CellAttrs::BOLD),
        "SGR bold restored"
    );
    assert_eq!(
        screen.active.pen_state.pen.fg,
        Color::Named(1),
        "SGR fg restored"
    );
    assert_eq!(
        screen.active.pen_state.pen.bg,
        Color::Named(2),
        "SGR bg restored"
    );
    assert!(
        screen.active.pen_state.pen.protected,
        "pen protected restored"
    );
    // ScrollRegion
    assert_eq!(
        screen.active.cursor.scroll_region.top, 1,
        "scroll region top restored"
    );
    assert_eq!(
        screen.active.cursor.scroll_region.bottom, 3,
        "scroll region bottom restored"
    );
    // Margins
    assert!(screen.active.cursor.margins.enabled, "margin mode restored");
    assert_eq!(screen.active.cursor.margins.left, 2, "margin left restored");
    assert_eq!(
        screen.active.cursor.margins.right, 9,
        "margin right restored"
    );
    // TerminalModes
    assert!(!screen.active.cursor.modes.autowrap, "autowrap restored");
    assert!(
        !screen.active.cursor.modes.pending_wrap,
        "pending wrap restored"
    );
    assert!(screen.active.cursor.modes.origin, "origin mode restored");
    assert!(screen.active.cursor.modes.insert, "insert mode restored");
    assert!(
        screen.active.cursor.modes.line_feed,
        "line feed mode restored"
    );
    assert!(
        screen.active.cursor.modes.application_cursor,
        "application cursor restored"
    );
    assert!(
        screen.active.cursor.modes.application_keypad,
        "application keypad restored"
    );
    // TabStops
    assert!(screen.active.pen_state.tab_stops.0[5], "tab stop restored");
    // CharacterSets
    assert_eq!(
        screen.active.pen_state.charsets.last_char,
        Some('X'),
        "last_char restored"
    );
    assert_eq!(
        screen.active.pen_state.charsets.g0, b'0',
        "g0 charset restored"
    );
    assert_eq!(
        screen.active.pen_state.charsets.g1, b'B',
        "g1 charset restored"
    );
    assert_eq!(
        screen.active.pen_state.charsets.active, 1,
        "active charset restored"
    );
    // Move cursor away, then restore_cursor proves saved cursor survived alt
    screen.active.cursor.cursor.x = 0;
    screen.active.cursor.cursor.y = 0;
    screen.restore_cursor();
    assert_eq!(screen.active.cursor.cursor.x, 5, "saved cursor x restored");
    assert_eq!(screen.active.cursor.cursor.y, 2, "saved cursor y restored");
}

#[test]
fn alt_screen_exit_marks_all_dirty() {
    let mut screen = Screen::new(5, 10);
    screen.clear_dirty();
    assert!(screen.dirty_ranges().is_empty());

    screen.enter_alt(true);
    screen.exit_alt();

    let ranges = screen.dirty_ranges();
    assert_eq!(ranges.len(), 5);
    for (i, range) in ranges.into_iter().enumerate() {
        assert_eq!(range.row, i);
        assert_eq!(range.start_col, 0);
        assert_eq!(range.end_col, 10);
    }
}

#[test]
fn alt_screen_transitions_preserve_pending_replies() {
    let mut screen = Screen::new(5, 10);
    screen.push_reply(b"before");

    screen.enter_alt(true);
    screen.push_reply(b"during");

    screen.exit_alt();

    assert_eq!(screen.drain_replies(), b"beforeduring");
}

#[test]
fn alt_screen_enter_without_clear_preserves_contents() {
    let mut screen = Screen::new(3, 10);
    screen.enter_alt(false);
    screen.cell_mut(0, 0).ch = 'A';
    screen.exit_alt();
    screen.enter_alt(false);
    assert_eq!(
        screen.row_text(0).trim(),
        "A",
        "?47-style re-entry keeps alternate contents"
    );
}

#[test]
fn alt_screen_reentry_exit_restores_primary() {
    let mut screen = Screen::new(3, 10);
    screen.cell_mut(0, 0).ch = 'P';
    screen.enter_alt(false);
    screen.cell_mut(0, 0).ch = 'A';
    screen.exit_alt();
    screen.enter_alt(false);
    screen.exit_alt();
    // Regression: the parked-buffer restore must not clobber the saved primary.
    assert_eq!(screen.row_text(0).trim(), "P");
    assert!(!screen.is_alt());
}

#[test]
fn alt_screen_enter_clear_drops_persisted_buffer() {
    let mut screen = Screen::new(3, 10);
    screen.enter_alt(false);
    screen.cell_mut(0, 0).ch = 'A';
    screen.exit_alt();
    screen.enter_alt(true);
    assert!(
        screen.row_text(0).trim().is_empty(),
        "?1047/?1049-style entry clears the alternate buffer"
    );
}

#[test]
fn alt_screen_exit_when_not_in_alt_keeps_primary() {
    let mut screen = Screen::new(3, 10);
    screen.cell_mut(0, 0).ch = 'A';
    screen.exit_alt();
    assert_eq!(screen.row_text(0).trim(), "A");
}

#[test]
fn alt_screen_exit_when_not_in_alt_preserves_content_anchors() {
    let mut screen = Screen::new(2, 4);
    screen.write_char('A');
    let (_, before) = screen.finish_anchor_mutations(None).unwrap();
    let anchor = before
        .to_anchor(GenPos::new(0, 0), Affinity::Before)
        .unwrap();

    screen.exit_alt();

    let (mutations, after) = screen.finish_anchor_mutations(Some(&before)).unwrap();
    let adjusted = mutations.apply(anchor).expect("anchor remains valid");
    assert_eq!(after.resolve_selection(adjusted), Some(GenPos::new(0, 0)));
}

#[test]
fn alt_screen_resize_while_parked_resizes_persisted_buffer() {
    // Arrange — park an alternate buffer with content (?47-style cycle).
    let mut screen = Screen::new(3, 10);
    screen.enter_alt(false);
    screen.cell_mut(0, 0).ch = 'A';
    screen.exit_alt();

    // Act — grow the primary; the parked alternate buffer must follow.
    screen.resize(5, 20);

    // Assert — re-entry installs the resized buffer with contents intact.
    screen.enter_alt(false);
    assert_eq!((screen.rows(), screen.cols()), (5, 20));
    assert_eq!(screen.row_text(0).trim(), "A");
}

#[test]
fn alt_screen_resize_while_parked_clamps_shrunk_buffer() {
    // Arrange — park an alternate buffer with a cell that will survive the
    // shrink and one that will be cut off.
    let mut screen = Screen::new(5, 20);
    screen.enter_alt(false);
    screen.cell_mut(0, 0).ch = 'A';
    screen.cell_mut(4, 19).ch = 'C';
    screen.exit_alt();

    // Act — shrink the primary; the parked buffer must shrink too.
    screen.resize(2, 5);

    // Assert — re-entry installs the clamped buffer; out-of-bounds cells dropped.
    screen.enter_alt(false);
    assert_eq!((screen.rows(), screen.cols()), (2, 5));
    assert_eq!(screen.row_text(0).trim(), "A");
    assert!(screen.row_text(1).trim().is_empty());
}

#[test]
fn reset_display_drops_parked_alt_buffer() {
    // Arrange — park an alternate buffer with content.
    let mut screen = Screen::new(3, 10);
    screen.enter_alt(false);
    screen.cell_mut(0, 0).ch = 'A';
    screen.exit_alt();
    assert!(!screen.is_alt());

    // Act — RIS-equivalent full display reset.
    screen.reset_display();

    // Assert — the parked buffer is gone, so a later ?47-style entry is blank.
    assert!(!screen.is_alt());
    screen.enter_alt(false);
    assert!(screen.row_text(0).trim().is_empty());
}

#[test]
fn alt_screen_repeated_cycles_keep_parking_alt_contents() {
    // Regression for the exit-side ordering: `exit_alt` parks the alternate
    // buffer only after the saved primary is restored (`*self = *primary`),
    // because the saved primary carries an empty `parked_alt` slot and would
    // clobber an earlier assignment. Two full cycles then a third re-entry
    // must still see the parked contents.
    let mut screen = Screen::new(3, 10);
    screen.cell_mut(0, 0).ch = 'P';

    // Arrange — two complete ?47-style cycles (enter → exit → enter → exit).
    screen.enter_alt(false);
    screen.cell_mut(0, 0).ch = 'A';
    screen.exit_alt();
    screen.enter_alt(false);
    screen.exit_alt();

    // Act — third re-entry, which needs the buffer parked by the second exit.
    screen.enter_alt(false);

    // Assert — the re-parked alternate contents survive the second exit.
    assert_eq!(
        screen.row_text(0).trim(),
        "A",
        "each exit must re-park the alternate buffer"
    );
    screen.exit_alt();
    assert_eq!(screen.row_text(0).trim(), "P");
    assert!(!screen.is_alt());
}

#[test]
fn alt_screen_reentry_marks_all_dirty() {
    // The parked-restore path must mark the whole screen dirty, mirroring
    // `alt_screen_exit_marks_all_dirty` for the exit.
    let mut screen = Screen::new(3, 10);
    screen.enter_alt(false);
    screen.cell_mut(0, 0).ch = 'A';
    screen.exit_alt();
    screen.clear_dirty();
    assert!(screen.dirty_ranges().is_empty());

    // Act — re-enter ?47-style, restoring the parked buffer.
    screen.enter_alt(false);

    // Assert — every row of the reinstalled buffer is dirty, full width.
    let ranges = screen.dirty_ranges();
    assert_eq!(ranges.len(), 3);
    for (i, range) in ranges.into_iter().enumerate() {
        assert_eq!(range.row, i);
        assert_eq!(range.start_col, 0);
        assert_eq!(range.end_col, 10);
    }
}

#[test]
fn alt_screen_reentry_cycle_preserves_pending_replies() {
    // Replies must survive the full park → restore cycle: `enter_alt` and
    // `exit_alt` both carry the pending reply buffer across the whole-screen
    // swap, including when a parked buffer is installed on re-entry.
    let mut screen = Screen::new(3, 10);
    screen.push_reply(b"a");

    screen.enter_alt(false);
    screen.push_reply(b"b");
    screen.exit_alt(); // parks the alternate buffer
    screen.push_reply(b"c");
    screen.enter_alt(false); // restores the parked buffer
    screen.push_reply(b"d");
    screen.exit_alt();

    assert_eq!(screen.drain_replies(), b"abcd");
}

#[test]
fn alt_screen_park_restore_keeps_snapshot_rows_and_advances_damage_revisions() {
    let mut screen = Screen::new(2, 8);
    screen.write_char('P');
    let primary_snapshot = screen.terminal_snapshot();
    let primary_generation = screen.active.normal.content_generation();
    let primary_revision = screen.active.normal.stamp();

    screen.enter_alt(false);
    assert_ne!(screen.active.normal.stamp(), primary_revision);
    screen.write_char('A');
    let alternate_snapshot = screen.terminal_snapshot();
    let alternate_generation = screen.active.normal.content_generation();
    screen.clear_dirty();
    let alternate_revision = screen.active.normal.stamp();

    screen.exit_alt();
    let primary_after_exit = screen.active.normal.stamp();
    assert_eq!(
        screen.active.normal.content_generation(),
        primary_generation
    );
    assert_ne!(primary_after_exit, primary_revision);
    let parked = screen.parked_alt.as_deref().unwrap();
    assert_eq!(parked.normal.stamp(), alternate_revision);
    assert_eq!(
        primary_snapshot
            .cells
            .shared_rows_with(&screen.terminal_snapshot().cells),
        2
    );

    let before_reentry = screen.mutation_revisions();
    screen.enter_alt(false);
    let after_reentry = screen.mutation_revisions();
    assert!(screen.parked_alt.is_none()); // saved primary and parked alternate are leaf types
    assert_eq!(
        screen.active.normal.content_generation(),
        alternate_generation
    );
    assert_ne!(screen.active.normal.stamp(), alternate_revision);
    assert_eq!(before_reentry[2], after_reentry[2]); // session colors
    assert_eq!(before_reentry[3], after_reentry[3]); // synchronized output
    assert_eq!(
        alternate_snapshot
            .cells
            .shared_rows_with(&screen.terminal_snapshot().cells),
        2
    );
    assert_eq!(screen.cell(0, 0).ch, 'A');

    screen.exit_alt();
    let after_second_exit = screen.mutation_revisions();
    assert_ne!(screen.active.normal.stamp(), primary_after_exit);
    assert_eq!(after_reentry[2], after_second_exit[2]);
    assert_eq!(after_reentry[3], after_second_exit[3]);
    assert_eq!(screen.cell(0, 0).ch, 'P');
    screen.enter_alt(true);
    assert_ne!(
        screen.active.normal.content_generation(),
        alternate_generation
    );
    assert_eq!(screen.cell(0, 0).ch, ' ');
    // The cleared buffer's already-issued snapshot still owns its rows.
    assert_eq!(alternate_snapshot.cells[0].ch, 'A');
    screen.exit_alt();
    screen.enter_alt(false);
    assert_eq!(screen.cell(0, 0).ch, ' ');
}

#[test]
fn alt_screen_exit_reconciles_saved_cursor_before_parking() {
    let mut screen = Screen::new(1, 10);
    screen.enter_alt(false);
    for ch in "abcdef".chars() {
        screen.write_char(ch);
    }
    screen.finish_anchor_mutations(None).unwrap();
    screen.active.cursor.cursor.x = 4;
    screen.save_cursor();

    screen.active.cursor.cursor.x = 1;
    screen.active.cursor.modes.insert = true;
    screen.write_char('X');
    screen.exit_alt();
    screen.enter_alt(false);
    screen.restore_cursor();

    assert_eq!((screen.cursor_x(), screen.cursor_y()), (5, 0));
}

#[test]
fn alt_screen_reentry_commits_deferred_primary_resize_before_parking() {
    let mut screen = Screen::new(3, 12);
    screen.write_char('P');
    screen.enter_alt(false);
    screen.write_char('A');
    screen.exit_alt();
    screen.enter_alt(false);

    for cols in [5, 8] {
        let prepared = screen
            .prepare_resize_with_viewport(3, cols, ReflowViewport::PreserveLiveTop)
            .unwrap();
        screen.commit_resize(prepared);
        assert_eq!(screen.saved_primary.as_ref().unwrap().cols(), 12);
        assert_eq!(screen.cell(0, 0).ch, 'A');
    }

    screen.exit_alt();
    assert_eq!((screen.rows(), screen.cols()), (3, 8));
    assert_eq!(screen.cell(0, 0).ch, 'P');
    assert!(screen.deferred_primary_resize.is_empty());
    let parked = screen.parked_alt.as_deref().unwrap();
    assert_eq!((parked.rows(), parked.cols()), (3, 8));

    screen.enter_alt(false);
    assert_eq!((screen.rows(), screen.cols()), (3, 8));
    assert_eq!(screen.cell(0, 0).ch, 'A');
    screen.exit_alt();
    assert_eq!(screen.cell(0, 0).ch, 'P');
}

#[test]
fn alt_screen_reentry_uses_current_session_state_not_parked_state() {
    let mut screen = Screen::new(2, 8);
    screen.set_default_color_rgb(DefaultColorSlot::Foreground, [0x12, 0x34, 0x56]);
    screen.set_indexed_color_rgb(200, [0x23, 0x45, 0x67]);
    screen.set_private_mode(1004, true);
    assert!(screen.observe_focus(crate::TerminalFocusEvent::Gained));
    screen.set_private_mode(2026, true);
    screen.push_reply(b"p");
    let initial_palette = screen.active_palette();

    screen.enter_alt(false);
    assert_eq!(screen.active_palette(), initial_palette);
    screen.set_default_color_rgb(DefaultColorSlot::Foreground, [0x34, 0x56, 0x78]);
    screen.set_indexed_color_rgb(200, [0x45, 0x67, 0x89]);
    assert!(screen.observe_focus(crate::TerminalFocusEvent::Lost));
    screen.set_private_mode(2026, true);
    screen.push_reply(b"a");
    let alternate_palette = screen.active_palette();

    screen.exit_alt();
    assert_eq!(screen.active_palette(), alternate_palette);
    assert!(!screen.observe_focus(crate::TerminalFocusEvent::Lost));
    assert!(!screen.ordinary_present_eligible());
    screen.set_default_color_rgb(DefaultColorSlot::Foreground, [0x56, 0x78, 0x9a]);
    screen.set_indexed_color_rgb(200, [0x67, 0x89, 0xab]);
    assert!(screen.observe_focus(crate::TerminalFocusEvent::Gained));
    screen.set_private_mode(2026, false);
    screen.push_reply(b"q");
    let current_palette = screen.active_palette();

    screen.enter_alt(false);
    assert_eq!(screen.active_palette(), current_palette);
    assert_eq!(screen.mode_status(true, 1004), ModeStatus::Set);
    assert!(!screen.observe_focus(crate::TerminalFocusEvent::Gained));
    assert!(screen.observe_focus(crate::TerminalFocusEvent::Lost));
    assert!(!screen.ordinary_present_eligible());
    screen.set_private_mode(2026, false);
    assert!(screen.ordinary_present_eligible());
    screen.push_reply(b"r");

    screen.exit_alt();
    assert_eq!(screen.active_palette(), current_palette);
    assert!(!screen.observe_focus(crate::TerminalFocusEvent::Lost));
    assert!(screen.ordinary_present_eligible());
    assert_eq!(screen.drain_replies(), b"paqr");
}

#[test]
fn alt_screen_enter_while_in_alt_never_clears() {
    // The in-alt guard must win over the `clear` flag: a ?1047/?1049-style
    // entry while already in the alternate screen is a no-op and must not
    // wipe the active buffer.
    let mut screen = Screen::new(3, 10);
    screen.enter_alt(true);
    screen.cell_mut(0, 0).ch = 'A';

    // Act — repeated clear-entry while already in alt.
    screen.enter_alt(true);

    // Assert — buffer intact and still in alt; exit restores the primary.
    assert!(screen.is_alt());
    assert_eq!(screen.row_text(0).trim(), "A");
    screen.exit_alt();
    assert!(!screen.is_alt());
    assert!(screen.row_text(0).trim().is_empty());
}

#[test]
fn alt_screen_reentry_restores_alt_cursor_position() {
    // The parked buffer carries its own cursor: ?47 re-entry must restore
    // the alternate cursor, never the primary's.
    let mut screen = Screen::new(5, 20);
    screen.set_cursor_position(1, 4); // 1-based → 0-based (y=0, x=3)
    screen.enter_alt(false);
    screen.set_cursor_position(3, 3); // 0-based (y=2, x=2)
    screen.exit_alt();
    assert_eq!(
        (screen.cursor_x(), screen.cursor_y()),
        (3, 0),
        "primary cursor restored on exit"
    );

    // Act — ?47-style re-entry.
    screen.enter_alt(false);

    // Assert — the alternate cursor was parked and restored with the buffer.
    assert_eq!(
        (screen.cursor_x(), screen.cursor_y()),
        (2, 2),
        "re-entry restores the alternate buffer's own cursor"
    );
    screen.exit_alt();
    assert_eq!(
        (screen.cursor_x(), screen.cursor_y()),
        (3, 0),
        "primary cursor survives the cycle"
    );
}

#[test]
fn reset_display_while_in_alt_drops_saved_and_parked_state() {
    // RIS while in alt must drop both the saved primary (`saved_primary`) and any
    // parked buffer (`parked_alt`): after the reset there is nothing to
    // restore on exit and nothing to resurrect on a later ?47 entry.
    let mut screen = Screen::new(3, 10);
    screen.enter_alt(true);
    screen.cell_mut(0, 0).ch = 'A';

    // Act — full display reset while in the alternate screen.
    screen.reset_display();

    // Assert — out of alt, blank; exit is a no-op and re-entry is blank.
    assert!(!screen.is_alt());
    assert!(screen.row_text(0).trim().is_empty());
    screen.exit_alt();
    assert!(!screen.is_alt());
    assert!(screen.row_text(0).trim().is_empty());
    screen.enter_alt(false);
    assert!(
        screen.row_text(0).trim().is_empty(),
        "no parked buffer may survive the reset"
    );
}

#[test]
fn alt_screen_1047_exit_then_47_reentry_restores_shared_buffer() {
    // ?47 and ?1047 share one alternate buffer: a clear-entry (?1047) parks
    // its contents on exit, and a later non-clear entry (?47) restores them.
    let mut screen = Screen::new(3, 10);
    screen.cell_mut(0, 0).ch = 'P';
    screen.enter_alt(true); // ?1047-style clear entry
    screen.cell_mut(0, 0).ch = 'A';
    screen.exit_alt();
    assert_eq!(screen.row_text(0).trim(), "P");

    // Act — ?47-style re-entry after a clear-mode exit.
    screen.enter_alt(false);

    // Assert — the buffer parked by the ?1047 exit is restored.
    assert_eq!(
        screen.row_text(0).trim(),
        "A",
        "?47 re-entry restores the buffer parked by the ?1047 exit"
    );
    screen.exit_alt();
    assert_eq!(screen.row_text(0).trim(), "P");
}

#[test]
fn mouse_tracking_modes_remain_buffer_local_across_alt_screen_swaps() {
    let mut screen = Screen::new(2, 8);
    screen.set_private_mode(1003, true);
    screen.set_private_mode(1006, true);

    screen.enter_alt(false);
    assert_eq!(screen.input_modes(), crate::InputModes::default());
    screen.set_private_mode(1000, true);

    screen.exit_alt();
    assert_eq!(
        screen.input_modes().mouse_tracking,
        crate::model::MouseTrackingMode::AnyMotion
    );
    assert!(screen.input_modes().mouse_sgr);

    screen.enter_alt(false);
    assert_eq!(
        screen.input_modes().mouse_tracking,
        crate::model::MouseTrackingMode::Button
    );
    assert!(!screen.input_modes().mouse_sgr);
}

#[test]
fn should_preserve_focus_reporting_state_across_alt_screen_swaps() {
    let mut screen = Screen::new(2, 8);
    screen.set_private_mode(1004, true);
    assert!(screen.observe_focus(crate::TerminalFocusEvent::Gained));

    screen.enter_alt(false);
    assert_eq!(screen.mode_status(true, 1004), ModeStatus::Set);
    assert!(!screen.observe_focus(crate::TerminalFocusEvent::Gained));
    assert!(screen.observe_focus(crate::TerminalFocusEvent::Lost));

    screen.exit_alt();
    assert_eq!(screen.mode_status(true, 1004), ModeStatus::Set);
    assert!(!screen.observe_focus(crate::TerminalFocusEvent::Lost));

    screen.enter_alt(false);
    assert!(!screen.observe_focus(crate::TerminalFocusEvent::Lost));
    assert!(screen.observe_focus(crate::TerminalFocusEvent::Gained));
    screen.exit_alt();
    assert!(!screen.observe_focus(crate::TerminalFocusEvent::Gained));
}

#[test]
fn should_preserve_ineligibility_when_alt_screen_swaps_at_nonzero_depth() {
    // Arrange
    let mut screen = Screen::new(2, 8);
    screen.set_private_mode(2026, true);

    // Act
    screen.enter_alt(true);
    let alt_status = screen.mode_status(true, 2026);
    let alt_eligible = screen.ordinary_present_eligible();
    screen.exit_alt();
    let primary_status = screen.mode_status(true, 2026);
    let primary_eligible = screen.ordinary_present_eligible();

    // Assert
    assert_eq!(alt_status, ModeStatus::Set);
    assert!(!alt_eligible);
    assert_eq!(primary_status, ModeStatus::Set);
    assert!(!primary_eligible);
}

#[test]
fn primary_and_alternate_screens_resolve_overlapping_ids_in_their_own_registries() {
    let mut screen = Screen::new(2, 4);
    screen.open_hyperlink("https://primary.test".to_owned(), None);
    screen.write_char('p');
    let primary_id = screen.cell(0, 0).hyperlink.unwrap();

    screen.enter_alt(false);
    screen.open_hyperlink("https://alternate.test".to_owned(), None);
    screen.write_char('a');
    let alternate_id = screen.cell(0, 0).hyperlink.unwrap();
    assert_eq!(
        primary_id, alternate_id,
        "screen-local ID spaces may overlap"
    );
    assert_eq!(
        screen
            .hyperlink_at_generation(screen.history_start(), 0)
            .map(|(_, uri)| uri),
        Some("https://alternate.test")
    );

    screen.exit_alt();
    assert_eq!(
        screen
            .hyperlink_at_generation(screen.history_start(), 0)
            .map(|(_, uri)| uri),
        Some("https://primary.test")
    );
    screen.enter_alt(false);
    assert_eq!(
        screen
            .hyperlink_at_generation(screen.history_start(), 0)
            .map(|(_, uri)| uri),
        Some("https://alternate.test")
    );
}

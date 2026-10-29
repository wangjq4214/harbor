use super::*;

#[test]
fn mouse_tracking_modes_are_independent_prioritized_and_reset_by_ris() {
    let mut screen = Screen::new(2, 8);

    for mode in [1000, 1002, 1003, 1006] {
        assert_eq!(screen.mode_status(true, mode), ModeStatus::Reset);
    }
    assert_eq!(
        screen.input_modes().mouse_tracking,
        crate::model::MouseTrackingMode::Disabled
    );

    for order in [
        [1000, 1002, 1003],
        [1000, 1003, 1002],
        [1002, 1000, 1003],
        [1002, 1003, 1000],
        [1003, 1000, 1002],
        [1003, 1002, 1000],
    ] {
        let mut ordered = Screen::new(2, 8);
        for mode in order {
            ordered.set_private_mode(mode, true);
        }
        assert_eq!(
            ordered.input_modes().mouse_tracking,
            crate::model::MouseTrackingMode::AnyMotion
        );
    }

    for mode in [1002, 1000, 1003, 1006] {
        screen.set_private_mode(mode, true);
    }
    assert_eq!(
        screen.input_modes().mouse_tracking,
        crate::model::MouseTrackingMode::AnyMotion
    );
    for mode in [1000, 1002, 1003, 1006] {
        assert_eq!(screen.mode_status(true, mode), ModeStatus::Set);
    }

    screen.set_private_mode(1000, false);
    assert_eq!(screen.mode_status(true, 1000), ModeStatus::Reset);
    assert_eq!(
        screen.input_modes().mouse_tracking,
        crate::model::MouseTrackingMode::AnyMotion
    );
    screen.set_private_mode(1003, false);
    assert_eq!(
        screen.input_modes().mouse_tracking,
        crate::model::MouseTrackingMode::ButtonMotion
    );

    screen.reset_display();
    assert_eq!(screen.input_modes(), crate::InputModes::default());
    for mode in [1000, 1002, 1003, 1006] {
        assert_eq!(screen.mode_status(true, mode), ModeStatus::Reset);
    }
}

#[test]
fn should_reset_focus_reporting_only_on_hard_reset() {
    let mut screen = Screen::new(2, 8);
    screen.set_private_mode(1004, true);
    assert!(screen.observe_focus(crate::TerminalFocusEvent::Lost));

    screen.soft_reset();
    assert_eq!(screen.mode_status(true, 1004), ModeStatus::Set);
    assert!(!screen.observe_focus(crate::TerminalFocusEvent::Lost));

    screen.reset_display();
    assert_eq!(screen.mode_status(true, 1004), ModeStatus::Reset);
    screen.set_private_mode(1004, true);
    assert!(
        screen.observe_focus(crate::TerminalFocusEvent::Lost),
        "RIS must clear the observed focus baseline"
    );
}

#[test]
fn should_be_eligible_when_synchronized_output_is_default() {
    // Arrange
    let screen = Screen::new(2, 8);

    // Act
    let eligible = screen.ordinary_present_eligible();
    let status = screen.mode_status(true, 2026);

    // Assert
    assert!(eligible);
    assert_eq!(status, ModeStatus::Reset);
    assert_ne!(status, ModeStatus::Unknown);
}

#[test]
fn should_be_ineligible_when_private_mode_2026_is_enabled() {
    // Arrange
    let mut screen = Screen::new(2, 8);

    // Act
    screen.set_private_mode(2026, true);

    // Assert
    assert!(!screen.ordinary_present_eligible());
    assert_eq!(screen.mode_status(true, 2026), ModeStatus::Set);
    assert_ne!(screen.mode_status(true, 2026), ModeStatus::Unknown);
}

#[test]
fn should_restore_eligibility_when_private_mode_2026_is_disabled() {
    // Arrange
    let mut screen = Screen::new(2, 8);
    screen.set_private_mode(2026, true);

    // Act
    screen.set_private_mode(2026, false);

    // Assert
    assert!(screen.ordinary_present_eligible());
    assert_eq!(screen.mode_status(true, 2026), ModeStatus::Reset);
}

#[test]
fn should_keep_set_when_one_disable_leaves_2026_nested() {
    // Arrange
    let mut screen = Screen::new(2, 8);
    screen.set_private_mode(2026, true);
    screen.set_private_mode(2026, true);

    // Act
    screen.set_private_mode(2026, false);

    // Assert
    assert!(!screen.ordinary_present_eligible());
    assert_eq!(screen.mode_status(true, 2026), ModeStatus::Set);
}

#[test]
fn should_keep_eligible_when_2026_is_disabled_at_zero() {
    // Arrange
    let mut screen = Screen::new(2, 8);

    // Act
    screen.set_private_mode(2026, false);
    screen.set_private_mode(2026, false);

    // Assert
    assert!(screen.ordinary_present_eligible());
    assert_eq!(screen.mode_status(true, 2026), ModeStatus::Reset);
}

#[test]
fn should_restore_eligibility_when_reset_display_clears_nested_2026() {
    // Arrange
    let mut screen = Screen::new(2, 8);
    screen.set_private_mode(2026, true);
    screen.set_private_mode(2026, true);
    screen.write_char('x');

    // Act
    screen.reset_display();

    // Assert
    assert!(screen.ordinary_present_eligible());
    assert_eq!(screen.mode_status(true, 2026), ModeStatus::Reset);
}

#[test]
fn should_keep_set_when_soft_reset_leaves_nested_2026() {
    // Arrange
    let mut screen = Screen::new(2, 8);
    screen.set_private_mode(2026, true);
    screen.write_char('x');

    // Act
    screen.soft_reset();

    // Assert
    assert!(!screen.ordinary_present_eligible());
    assert_eq!(screen.mode_status(true, 2026), ModeStatus::Set);
    assert!(screen.row_text(0).contains('x'));
}

#[test]
fn should_keep_cells_when_clear_synchronized_output_releases_suppression() {
    // Arrange
    let mut screen = Screen::new(2, 8);
    screen.set_private_mode(2026, true);
    screen.write_char('x');

    // Act
    screen.clear_synchronized_output();

    // Assert
    assert!(screen.ordinary_present_eligible());
    assert_eq!(screen.mode_status(true, 2026), ModeStatus::Reset);
    assert!(screen.row_text(0).contains('x'));
}

#[test]
fn should_keep_eligible_when_reset_display_while_already_reset() {
    // Arrange
    let mut screen = Screen::new(2, 8);

    // Act
    screen.reset_display();

    // Assert
    assert!(screen.ordinary_present_eligible());
    assert_eq!(screen.mode_status(true, 2026), ModeStatus::Reset);
}

#[test]
fn should_keep_set_when_soft_reset_leaves_depth_two_2026() {
    // Arrange
    let mut screen = Screen::new(2, 8);
    screen.set_private_mode(2026, true);
    screen.set_private_mode(2026, true);
    screen.write_char('x');

    // Act
    screen.soft_reset();

    // Assert
    assert!(!screen.ordinary_present_eligible());
    assert_eq!(screen.mode_status(true, 2026), ModeStatus::Set);
    assert!(screen.row_text(0).contains('x'));
}

#[test]
fn should_keep_cells_when_clear_synchronized_output_releases_nested_suppression() {
    // Arrange
    let mut screen = Screen::new(2, 8);
    screen.set_private_mode(2026, true);
    screen.set_private_mode(2026, true);
    screen.write_char('x');

    // Act
    screen.clear_synchronized_output();

    // Assert
    assert!(screen.ordinary_present_eligible());
    assert_eq!(screen.mode_status(true, 2026), ModeStatus::Reset);
    assert!(screen.row_text(0).contains('x'));
}

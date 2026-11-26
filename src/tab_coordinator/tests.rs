mod action_batch_tests;
mod configuration_reload_tests;
#[cfg(windows)]
mod pane_lifecycle_tests;

use super::*;

#[test]
fn logical_width_conversion_handles_dpi_zero_and_invalid_scale() {
    assert_eq!(logical_width_from_physical(1_350, 1.5), 900.0);
    assert_eq!(logical_width_from_physical(0, 2.0), 0.0);
    assert_eq!(logical_width_from_physical(900, 0.0), 0.0);
    assert_eq!(logical_width_from_physical(900, f64::NAN), 0.0);
    assert_eq!(logical_width_from_physical(900, f64::INFINITY), 0.0);
}

#[test]
fn native_window_title_appends_product_suffix() {
    assert_eq!(format_window_title("pwsh.exe"), "pwsh.exe — Harbor");
    assert_eq!(
        format_window_title("project — Harbor"),
        "project — Harbor — Harbor"
    );
}

#[test]
fn tab_drain_outcome_defaults_to_not_closing() {
    let outcome = TabDrainOutcome::default();
    assert!(!outcome.close_window);
}

#[test]
fn confirmation_controller_uses_in_window_overlay_without_secondary_host() {
    let production = include_str!("../../dialog.rs");
    for legacy in [
        "ConfirmationWindow",
        "WinitWindowHostBuilder",
        "WindowAttributesExtWindows",
        "WindowSurface",
        "WinitAdapter",
        "WinitFrameTarget",
        "apply_window_effects",
        "apply_control_flow",
        "create_transitional_child_runtime",
    ] {
        assert!(
            !production.contains(legacy),
            "confirmation production code must not reference secondary-window or legacy host infrastructure: {legacy}"
        );
    }
}

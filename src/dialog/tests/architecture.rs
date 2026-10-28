#[test]
fn confirmation_production_code_owns_only_the_common_native_host() {
    let production = include_str!("../../dialog.rs");
    for legacy in [
        "WindowSurface",
        "WinitAdapter",
        "WinitFrameTarget",
        "apply_window_effects",
        "apply_control_flow",
        "create_transitional_child_runtime",
    ] {
        assert!(
            !production.contains(legacy),
            "confirmation production code must not reference legacy host infrastructure: {legacy}"
        );
    }
    assert!(production.contains("host: WinitWindowHost"));
}

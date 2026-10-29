#[test]
fn shell_production_code_owns_only_the_common_native_host() {
    let production = include_str!("../../shell.rs");
    for legacy in [
        "WindowSurface",
        "WinitAdapter",
        "WinitFrameTarget",
        "create_transitional_child_runtime",
        "SurfaceConfiguration",
    ] {
        assert!(
            !production.contains(legacy),
            "shell production code must not reference legacy host infrastructure: {legacy}"
        );
    }
    assert!(production.contains("main_host: WinitWindowHost"));
}

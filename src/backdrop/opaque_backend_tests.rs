#[path = "opaque_backend_tests/native.rs"]
mod native;

use super::*;

#[test]
fn main_window_hooks_preserve_hidden_startup_attributes() {
    let hooks =
        MainWindowPlatformHooks::new(Box::new(OpaqueBackend), WindowBackdropStyle::default());
    let attributes = hooks
        .configure_attributes(Window::default_attributes())
        .expect("configure main window attributes");

    assert_eq!(attributes.title, "Harbor");
    assert!(!attributes.visible);
    assert_eq!(attributes.preferred_theme, Some(Theme::Dark));
    assert!(attributes.window_icon.is_some());
}

#[test]
fn should_select_opaque_tier_when_probe_succeeds_on_unknown_build() {
    // Arrange — a successful WASDK probe must not override a failed OS probe.
    let build = 0;
    let wasdk_ok = true;

    // Act
    let tier = selected_tier(build, wasdk_ok);

    // Assert — plan 0018 explicitly requires (build=0, any) -> Opaque.
    assert_eq!(tier, BackdropTier::OpaqueFallback);
}

#[test]
fn should_keep_window_opaque_when_selected_for_unknown_build() {
    for wasdk_ok in [false, true] {
        // Arrange — exercise the boxed selector, not just its classification.
        let backend = select_backend(0, wasdk_ok);
        let attributes = Window::default_attributes()
            .with_title("unknown build")
            .with_visible(false);

        // Act
        let configured = backend.configure_attributes(attributes);

        // Assert
        assert!(!configured.transparent, "wasdk_ok={wasdk_ok}");
        assert!(!configured.visible);
        assert_eq!(configured.title, "unknown build");
    }
}

#[test]
fn should_preserve_window_attributes_when_opaque_backend_configures_them() {
    // Arrange
    let mut attributes = Window::default_attributes();
    attributes.title = "opaque fallback".to_owned();
    attributes.resizable = false;
    attributes.visible = false;
    attributes.transparent = true;
    attributes.blur = true;
    attributes.decorations = false;
    attributes.content_protected = true;
    attributes.active = false;

    // Act
    let configured = OpaqueBackend.configure_attributes(attributes);

    // Assert
    assert_eq!(configured.title, "opaque fallback");
    assert!(!configured.resizable);
    assert!(!configured.visible);
    assert!(configured.transparent);
    assert!(configured.blur);
    assert!(!configured.decorations);
    assert!(configured.content_protected);
    assert!(!configured.active);
}

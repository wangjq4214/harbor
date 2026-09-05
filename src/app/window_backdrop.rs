//! Platform/version-agnostic main-window backdrop backends (ADR 0027).
//!
//! The host selects one backend once at bootstrap through [`select_backend`]
//! and then only talks to the boxed [`WindowBackdropBackend`] contract. Every
//! Windows version detail (Windows App SDK runtime, accent policy, DWM state)
//! stays inside this module and its `wasdk` submodule.
//!
//! Two window responsibilities remain host-level by design (ADR 0024/0027):
//! caption suppression ([`super::suppress_caption_title_and_icon`]) and the
//! pre-surface GDI fallback paint, both of which are window chrome rather
//! than backdrop composition.

#[cfg(target_os = "windows")]
pub(crate) mod wasdk;

#[cfg(target_os = "windows")]
use std::cell::RefCell;

use harbor_config::WindowBackdropStyle;
use winit::window::{Window, WindowAttributes};

/// Which tier of the three-tier backdrop chain (ADR 0026) is active.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BackdropTier {
    /// Windows App SDK `DesktopAcrylicController` acrylic.
    WasdkAcrylic,
    /// Accent-policy acrylic.
    AccentPolicyAcrylic,
    /// No compositor backdrop.
    OpaqueFallback,
}

/// The result of applying a backdrop, consumed by the host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BackdropStatus {
    pub(crate) tier: BackdropTier,
    pub(crate) backdrop_available: bool,
}

/// Capability: apply the unified window backdrop tint.
///
/// [`configure_attributes`](Self::configure_attributes) runs before window
/// creation; [`apply`](Self::apply) runs after the window exists.
pub(crate) trait WindowBackdropBackend {
    fn configure_attributes(&self, attrs: WindowAttributes) -> WindowAttributes;
    fn apply(&self, window: &Window, style: &WindowBackdropStyle) -> BackdropStatus;

    /// A shared DirectComposition visual owned by the backend, when the
    /// backdrop owns the window's topmost composition target and the
    /// renderer must present into it instead of creating its own.
    fn composition_visual(&self) -> Option<*mut core::ffi::c_void> {
        None
    }
}

/// Selects a backend for the given OS facts without touching any platform API.
pub(crate) fn select_backend(build: u32, wasdk_ok: bool) -> Box<dyn WindowBackdropBackend> {
    match selected_tier(build, wasdk_ok) {
        BackdropTier::WasdkAcrylic => {
            #[cfg(target_os = "windows")]
            {
                return Box::new(WasdkAcrylicBackend::new());
            }
            #[cfg(not(target_os = "windows"))]
            unreachable!("wasdk tier is unreachable outside Windows")
        }
        BackdropTier::AccentPolicyAcrylic => {
            #[cfg(target_os = "windows")]
            {
                return Box::new(AccentPolicyBackend);
            }
            #[cfg(not(target_os = "windows"))]
            unreachable!("accent tier is unreachable outside Windows")
        }
        BackdropTier::OpaqueFallback => Box::new(OpaqueBackend),
    }
}

/// Pure tier classification shared by the selector and its tests.
pub(crate) fn selected_tier(build: u32, wasdk_ok: bool) -> BackdropTier {
    #[cfg(target_os = "windows")]
    {
        if build == 0 {
            return BackdropTier::OpaqueFallback;
        }
        if wasdk_ok {
            return BackdropTier::WasdkAcrylic;
        }
        if build != 0 {
            return BackdropTier::AccentPolicyAcrylic;
        }
    }
    #[cfg(not(target_os = "windows"))]
    let _ = (build, wasdk_ok);
    BackdropTier::OpaqueFallback
}

/// Probes the Windows App SDK runtime and reports tier-1 availability.
pub(crate) fn wasdk_available() -> bool {
    #[cfg(target_os = "windows")]
    {
        wasdk::try_initialize()
    }
    #[cfg(not(target_os = "windows"))]
    {
        false
    }
}

/// Cross-platform OS build probe; `0` outside Windows or when probing fails.
pub(crate) fn os_build() -> u32 {
    #[cfg(target_os = "windows")]
    {
        windows_os_build()
    }
    #[cfg(not(target_os = "windows"))]
    {
        0
    }
}

#[cfg(target_os = "windows")]
pub(crate) fn windows_os_build() -> u32 {
    #[repr(C)]
    struct OsVersionInfoW {
        dw_os_version_info_size: u32,
        dw_major_version: u32,
        dw_minor_version: u32,
        dw_build_number: u32,
        dw_platform_id: u32,
        sz_csd_version: [u16; 128],
    }

    #[link(name = "ntdll")]
    unsafe extern "system" {
        fn RtlGetVersion(info: *mut OsVersionInfoW) -> i32;
    }

    let mut info = OsVersionInfoW {
        dw_os_version_info_size: std::mem::size_of::<OsVersionInfoW>() as u32,
        dw_major_version: 0,
        dw_minor_version: 0,
        dw_build_number: 0,
        dw_platform_id: 0,
        sz_csd_version: [0; 128],
    };
    // STATUS_SUCCESS == 0
    let status = unsafe { RtlGetVersion(&mut info) };
    if status == 0 { info.dw_build_number } else { 0 }
}

/// Packs the style tint into the ABGR `GradientColor` dword for accent policy.
#[cfg(target_os = "windows")]
fn style_tint_abgr(style: &WindowBackdropStyle) -> u32 {
    let to_byte = |channel: f32| (channel.clamp(0.0, 1.0) * 255.0).round() as u32;
    let r = to_byte(style.tint_rgb[0]);
    let g = to_byte(style.tint_rgb[1]);
    let b = to_byte(style.tint_rgb[2]);
    let a = to_byte(style.tint_opacity);
    (a << 24) | (b << 16) | (g << 8) | r
}

// ── Tier 1: Windows App SDK DesktopAcrylicController ────────────────────────

/// Keeps the controller and its desktop target alive for the window lifetime.
#[cfg(target_os = "windows")]
pub(crate) struct WasdkAcrylicBackend {
    state: RefCell<Option<wasdk::AcrylicState>>,
}

#[cfg(target_os = "windows")]
impl WasdkAcrylicBackend {
    pub(crate) fn new() -> Self {
        Self {
            state: RefCell::new(None),
        }
    }
}

#[cfg(target_os = "windows")]
impl WindowBackdropBackend for WasdkAcrylicBackend {
    fn configure_attributes(&self, attrs: WindowAttributes) -> WindowAttributes {
        use winit::platform::windows::WindowAttributesExtWindows;
        attrs
            .with_transparent(true)
            .with_no_redirection_bitmap(true)
    }

    fn apply(&self, window: &Window, style: &WindowBackdropStyle) -> BackdropStatus {
        let frame_extended = extend_dwm_frame_into_client_area(window);
        let controller_applied = frame_extended
            && match wasdk::apply_controller(window, style) {
                Ok(state) => {
                    *self.state.borrow_mut() = Some(state);
                    true
                }
                Err(error) => {
                    tracing::warn!(
                        %error,
                        "Windows App SDK acrylic failed; falling back to accent policy"
                    );
                    false
                }
            };
        if controller_applied {
            return BackdropStatus {
                tier: BackdropTier::WasdkAcrylic,
                backdrop_available: true,
            };
        }
        let accent_available = apply_accent_policy(window, style);
        BackdropStatus {
            tier: if accent_available {
                BackdropTier::AccentPolicyAcrylic
            } else {
                BackdropTier::OpaqueFallback
            },
            backdrop_available: accent_available,
        }
    }

    fn composition_visual(&self) -> Option<*mut core::ffi::c_void> {
        self.state.borrow().as_ref().map(|state| state.visual_ptr())
    }
}

// ── Tier 2: accent-policy acrylic ───────────────────────────────────────────

/// Windows 10 accent-policy acrylic migrated from the old app.rs bootstrap.
#[cfg(target_os = "windows")]
pub(crate) struct AccentPolicyBackend;

#[cfg(target_os = "windows")]
impl WindowBackdropBackend for AccentPolicyBackend {
    fn configure_attributes(&self, attrs: WindowAttributes) -> WindowAttributes {
        use winit::platform::windows::WindowAttributesExtWindows;
        attrs
            .with_transparent(true)
            .with_no_redirection_bitmap(true)
    }

    fn apply(&self, window: &Window, style: &WindowBackdropStyle) -> BackdropStatus {
        let accent_available = apply_accent_policy(window, style);
        BackdropStatus {
            tier: if accent_available {
                BackdropTier::AccentPolicyAcrylic
            } else {
                BackdropTier::OpaqueFallback
            },
            backdrop_available: accent_available,
        }
    }
}

/// Applies tier-2 accent-policy acrylic and reports whether it is live.
#[cfg(target_os = "windows")]
fn apply_accent_policy(window: &Window, style: &WindowBackdropStyle) -> bool {
    extend_dwm_frame_into_client_area(window)
        && apply_acrylic_accent_backdrop(window, style_tint_abgr(style))
        && dwm_composition_enabled()
}

/// Extends the DWM frame through the client area so transparent
/// DirectComposition pixels reveal the configured system backdrop.
#[cfg(target_os = "windows")]
fn extend_dwm_frame_into_client_area(window: &Window) -> bool {
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

    #[repr(C)]
    struct Margins {
        left: i32,
        right: i32,
        top: i32,
        bottom: i32,
    }

    #[link(name = "dwmapi")]
    unsafe extern "system" {
        fn DwmExtendFrameIntoClientArea(hwnd: isize, margins: *const Margins) -> i32;
    }

    let Ok(handle) = window.window_handle() else {
        tracing::warn!("DWM frame extension skipped: window handle unavailable");
        return false;
    };
    let RawWindowHandle::Win32(handle) = handle.as_raw() else {
        tracing::warn!("DWM frame extension skipped: non-Win32 window handle");
        return false;
    };
    let margins = Margins {
        left: -1,
        right: -1,
        top: -1,
        bottom: -1,
    };
    let result = unsafe { DwmExtendFrameIntoClientArea(handle.hwnd.get(), &margins) };
    if result != 0 {
        tracing::warn!(result, "DwmExtendFrameIntoClientArea failed");
        false
    } else {
        true
    }
}

/// Reports whether the Desktop Window Manager composition service is active.
#[cfg(target_os = "windows")]
fn dwm_composition_enabled() -> bool {
    #[link(name = "dwmapi")]
    unsafe extern "system" {
        fn DwmIsCompositionEnabled(enabled: *mut i32) -> i32;
    }

    let mut enabled = 0_i32;
    let result = unsafe { DwmIsCompositionEnabled(&mut enabled) };
    result == 0 && enabled != 0
}

/// `WCA_ACCENT_POLICY` attribute for `SetWindowCompositionAttribute`.
#[cfg(target_os = "windows")]
const WCA_ACCENT_POLICY: u32 = 19;
/// Enables acrylic blur behind the window via accent policy.
#[cfg(target_os = "windows")]
const ACCENT_ENABLE_ACRYLICBLURBEHIND: u32 = 4;

/// Applies accent-policy Acrylic on `window` using the packed tint gradient.
///
/// Missing HWND, an absent export, or API failure is logged and ignored.
#[cfg(target_os = "windows")]
fn apply_acrylic_accent_backdrop(window: &Window, gradient_abgr: u32) -> bool {
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

    #[repr(C)]
    struct AccentPolicy {
        accent_state: u32,
        accent_flags: u32,
        gradient_color: u32,
        animation_id: u32,
    }

    #[repr(C)]
    struct WindowCompositionAttribData {
        attrib: u32,
        pv_data: *mut AccentPolicy,
        cb_data: usize,
    }

    type SetWindowCompositionAttributeFn =
        unsafe extern "system" fn(isize, *mut WindowCompositionAttribData) -> i32;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetModuleHandleW(module_name: *const u16) -> isize;
        fn GetProcAddress(module: isize, proc_name: *const std::ffi::c_char) -> *const ();
    }

    let Ok(handle) = window.window_handle() else {
        tracing::warn!("accent acrylic skipped: window handle unavailable");
        return false;
    };
    let RawWindowHandle::Win32(h) = handle.as_raw() else {
        tracing::warn!("accent acrylic skipped: non-Win32 window handle");
        return false;
    };

    let hwnd = h.hwnd.get();
    let user32 = unsafe {
        GetModuleHandleW(
            [
                b'u' as u16,
                b's' as u16,
                b'e' as u16,
                b'r' as u16,
                b'3' as u16,
                b'2' as u16,
                b'.' as u16,
                b'd' as u16,
                b'l' as u16,
                b'l' as u16,
                0,
            ]
            .as_ptr(),
        )
    };
    if user32 == 0 {
        tracing::warn!("accent acrylic skipped: user32.dll unavailable");
        return false;
    }

    let proc = unsafe { GetProcAddress(user32, c"SetWindowCompositionAttribute".as_ptr()) };
    if proc.is_null() {
        tracing::warn!("accent acrylic skipped: SetWindowCompositionAttribute unavailable");
        return false;
    }
    let set_window_composition_attribute: SetWindowCompositionAttributeFn =
        unsafe { std::mem::transmute(proc) };

    let mut policy = AccentPolicy {
        accent_state: ACCENT_ENABLE_ACRYLICBLURBEHIND,
        accent_flags: 0,
        gradient_color: gradient_abgr,
        animation_id: 0,
    };
    let mut data = WindowCompositionAttribData {
        attrib: WCA_ACCENT_POLICY,
        pv_data: &mut policy,
        cb_data: std::mem::size_of::<AccentPolicy>(),
    };
    let ok = unsafe { set_window_composition_attribute(hwnd, &mut data) };
    if ok == 0 {
        tracing::warn!("SetWindowCompositionAttribute failed for accent acrylic");
        false
    } else {
        true
    }
}

// ── Tier 3 / non-Windows: opaque no-op ───────────────────────────────────────

/// Performs no compositor work; the host paints the opaque fallback itself.
pub(crate) struct OpaqueBackend;

impl WindowBackdropBackend for OpaqueBackend {
    fn configure_attributes(&self, attrs: WindowAttributes) -> WindowAttributes {
        attrs
    }

    fn apply(&self, _window: &Window, _style: &WindowBackdropStyle) -> BackdropStatus {
        BackdropStatus {
            tier: BackdropTier::OpaqueFallback,
            backdrop_available: false,
        }
    }
}

#[cfg(test)]
mod opaque_backend_tests {
    use super::*;

    #[cfg(target_os = "windows")]
    #[test]
    #[allow(deprecated)] // Hidden native fixture without entering a running event loop.
    fn should_report_opaque_fallback_when_applied_to_a_real_hidden_window() {
        use winit::event_loop::EventLoop;
        use winit::platform::windows::EventLoopBuilderExtWindows;

        // Arrange — exactly one event loop; never manufacture a Window reference.
        let event_loop = EventLoop::builder()
            .with_any_thread(true)
            .build()
            .expect("Windows event loop for hidden-window regression");
        let window = event_loop
            .create_window(Window::default_attributes().with_visible(false))
            .expect("real hidden Windows window");
        let backend = OpaqueBackend;
        let style = WindowBackdropStyle::default();

        // Act
        let status = backend.apply(&window, &style);

        // Assert
        assert_eq!(
            status,
            BackdropStatus {
                tier: BackdropTier::OpaqueFallback,
                backdrop_available: false,
            }
        );
        assert_eq!(window.is_visible(), Some(false));
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
    fn should_return_no_shared_visual_when_backend_has_not_been_applied() {
        for build in [0, 19_045, 22_621, 26_100] {
            for wasdk_ok in [false, true] {
                // Arrange — selection is pure and must not create native resources.
                let backend = select_backend(build, wasdk_ok);

                // Act
                let visual = backend.composition_visual();

                // Assert — the GPU must not receive an uninitialized visual.
                assert!(visual.is_none(), "build={build}, wasdk_ok={wasdk_ok}");
            }
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
}

#[cfg(all(test, target_os = "windows"))]
mod tests {
    use super::*;

    #[test]
    fn should_select_wasdk_backend_when_probe_succeeds() {
        for build in [1, 19_045, 22_621, 26_100, u32::MAX] {
            assert_eq!(
                selected_tier(build, true),
                BackdropTier::WasdkAcrylic,
                "build {build}"
            );
        }
    }

    #[test]
    fn should_select_accent_backend_for_known_builds_without_wasdk() {
        for build in [1, 19_045, 22_621, 26_100, u32::MAX] {
            assert_eq!(
                selected_tier(build, false),
                BackdropTier::AccentPolicyAcrylic,
                "build {build}"
            );
        }
    }

    #[test]
    fn should_select_opaque_backend_when_probe_fails_on_unknown_build() {
        assert_eq!(
            selected_tier(0, false),
            BackdropTier::OpaqueFallback,
            "unknown build without WASDK falls to opaque"
        );
    }

    #[test]
    fn should_prefer_wasdk_tier_for_known_builds() {
        // Unknown builds remain opaque per the plan's failure matrix.
        for build in [1, 19_045, 26_100, u32::MAX] {
            assert_eq!(
                selected_tier(build, true),
                BackdropTier::WasdkAcrylic,
                "build {build}"
            );
        }
    }

    #[test]
    fn should_enable_transparency_without_changing_chrome_when_acrylic_backend_is_selected() {
        for build in [1, 19_045, 22_620, 22_621, 26_100, u32::MAX] {
            for wasdk_ok in [false, true] {
                // Arrange
                let backend = select_backend(build, wasdk_ok);
                let attributes = Window::default_attributes()
                    .with_title("Harbor")
                    .with_theme(Some(winit::window::Theme::Dark))
                    .with_visible(false);

                // Act
                let configured = backend.configure_attributes(attributes);

                // Assert — both tiers need transparency, but startup stays hidden.
                assert!(configured.transparent, "build={build}, wasdk_ok={wasdk_ok}");
                assert!(!configured.visible);
                assert!(configured.decorations);
                assert_eq!(configured.title, "Harbor");
                assert_eq!(configured.preferred_theme, Some(winit::window::Theme::Dark));
            }
        }
    }

    #[test]
    fn should_pack_transparent_black_when_style_tint_is_zero() {
        // Arrange
        let style = WindowBackdropStyle {
            tint_rgb: [0.0, 0.0, 0.0],
            tint_opacity: 0.0,
            ..WindowBackdropStyle::default()
        };

        // Act
        let abgr = style_tint_abgr(&style);

        // Assert
        assert_eq!(abgr, 0x00_00_00_00);
    }

    #[test]
    fn should_clamp_alpha_to_zero_when_tint_opacity_is_negative() {
        // Arrange
        let style = WindowBackdropStyle {
            tint_rgb: [0.25, 0.5, 0.75],
            tint_opacity: -0.1,
            ..WindowBackdropStyle::default()
        };

        // Act
        let abgr = style_tint_abgr(&style);

        // Assert — alpha clamps independently; RGB is not premultiplied.
        assert_eq!(abgr, 0x00_BF_80_40);
    }

    #[test]
    fn should_pack_only_tint_when_fallback_and_luminosity_are_custom() {
        // Arrange
        let style = WindowBackdropStyle {
            fallback: [0.25, 0.5, 0.75],
            luminosity_opacity: 0.0,
            ..WindowBackdropStyle::default()
        };

        // Act
        let abgr = style_tint_abgr(&style);

        // Assert — non-tint style values must not leak into the AccentPolicy gradient.
        assert_eq!(abgr, 0x0F_FF_FF_FF);
    }

    #[test]
    fn should_pack_style_tint_into_abgr_for_accent_policy() {
        // White at 0.06 opacity → R=255 G=255 B=255 A=15
        let abgr = style_tint_abgr(&WindowBackdropStyle::default());
        assert_eq!(abgr, 0x0F_FF_FF_FF);
    }

    #[test]
    fn should_clamp_channels_when_packing_style_tint() {
        let style = WindowBackdropStyle {
            tint_rgb: [1.5, -0.1, 0.5],
            tint_opacity: 2.0,
            ..WindowBackdropStyle::default()
        };
        // Clamped to 255, 0, 128, 255
        assert_eq!(style_tint_abgr(&style), 0xFF_80_00_FF);
    }

    #[test]
    fn should_round_tint_channels_to_nearest_byte() {
        let style = WindowBackdropStyle {
            tint_rgb: [0.5, 0.5, 0.5],
            tint_opacity: 0.5,
            ..WindowBackdropStyle::default()
        };
        assert_eq!(style_tint_abgr(&style), 0x80_80_80_80);
    }

    #[test]
    fn should_pack_distinct_style_channels_in_abgr_order() {
        // Arrange — R=64, G=128, B=191, A=64.
        let style = WindowBackdropStyle {
            tint_rgb: [0.25, 0.5, 0.75],
            tint_opacity: 0.25,
            ..WindowBackdropStyle::default()
        };

        // Act
        let abgr = style_tint_abgr(&style);

        // Assert
        assert_eq!(abgr, 0x40_BF_80_40);
    }
}

#[cfg(all(test, not(target_os = "windows")))]
mod tests {
    use super::*;

    #[test]
    fn should_always_select_opaque_backend_outside_windows() {
        for build in [0, 19_045, 26_100] {
            for wasdk_ok in [false, true] {
                assert_eq!(
                    selected_tier(build, wasdk_ok),
                    BackdropTier::OpaqueFallback,
                    "build {build} wasdk_ok {wasdk_ok}"
                );
            }
        }
    }
}

//! Windows App SDK tier-1 wiring (ADR 0026).
//!
//! `generated.rs` is vendored from the pinned Windows App SDK 1.8 metadata;
//! regenerate it with `.tools/wasdk/gen` (see the generator's module docs).

mod generated;
mod interop;

use std::sync::OnceLock;

use harbor_config::WindowBackdropStyle;
use windows::System::DispatcherQueueController;
use windows::Win32::Foundation::HWND;
use windows::Win32::System::WinRT::{
    CreateDispatcherQueueController, DQTAT_COM_NONE, DQTYPE_THREAD_CURRENT, DispatcherQueueOptions,
};
use windows_core::Interface;
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::Window;

/// Resources that must stay alive while the acrylic controller is applied.
pub(crate) struct AcrylicState {
    _controller: generated::DesktopAcrylicController,
    _target: windows::UI::Composition::Desktop::DesktopWindowTarget,
    _container: windows::UI::Composition::ContainerVisual,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum WasdkError {
    #[error("window handle unavailable")]
    NoWindowHandle,
    #[error("non-Win32 window handle")]
    NonWin32Window,
    #[error("Windows App SDK runtime is not initialized")]
    NotInitialized,
    #[error("composition API failed: {0}")]
    Composition(#[from] windows_core::Error),
    #[error("host backdrop brush attribute failed: {0}")]
    HostBackdropBrush(i32),
    #[error("controller rejected the backdrop target")]
    TargetRejected,
}

struct WasdkRuntime {
    _dispatcher: DispatcherQueueController,
    window_id_interop: interop::WindowIdInterop,
}

static RUNTIME: OnceLock<WasdkRuntime> = OnceLock::new();

/// Initializes the Windows App SDK bootstrap, a DispatcherQueue, and probes
/// `DesktopAcrylicController::IsSupported()`. Returns false on any failure so
/// the selector falls back to the best OS-provided acrylic tier.
pub(crate) fn try_initialize() -> bool {
    if RUNTIME.get().is_some() {
        return true;
    }
    initialize_com_apartment();

    if !initialize_bootstrap() {
        tracing::warn!("Windows App SDK bootstrap unavailable; using OS acrylic fallback");
        return false;
    }
    let Ok(dispatcher) = create_dispatcher_queue() else {
        tracing::warn!("dispatcher queue creation failed; using OS acrylic fallback");
        return false;
    };
    let supported = generated::DesktopAcrylicController::IsSupported().unwrap_or(false);
    if !supported {
        tracing::warn!("DesktopAcrylicController unsupported; using OS acrylic fallback");
        return false;
    }
    let window_id_interop = match interop::WindowIdInterop::load() {
        Ok(interop) => interop,
        Err(error) => {
            tracing::warn!(%error, "WindowId interop unavailable; using OS acrylic fallback");
            return false;
        }
    };
    let _ = RUNTIME.set(WasdkRuntime {
        _dispatcher: dispatcher,
        window_id_interop,
    });
    true
}
/// Initializes COM and WinRT on the calling thread before the WASDK probe.
///
/// The bootstrap API and WinRT activation require an initialized apartment;
/// the winit event loop initializes OLE only during window creation, which
/// happens after this probe runs. A changed-mode result is acceptable.
fn initialize_com_apartment() {
    use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
    use windows::Win32::System::WinRT::{RO_INIT_SINGLETHREADED, RoInitialize};

    const RPC_E_CHANGED_MODE: i32 = -2_147_417_058; // 0x80010106
    let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.0;
    if !(hr == 0 || hr == 1 || hr == RPC_E_CHANGED_MODE) {
        tracing::warn!(hr, "CoInitializeEx failed");
    }
    // Best-effort WinRT initialization; a changed thread mode surfaces as a
    // probe failure below rather than a fatal error.
    let _ = unsafe { RoInitialize(RO_INIT_SINGLETHREADED) };
}

/// The desktop composition target layer for the backdrop material is always the lower slot.
pub(crate) const BACKDROP_TARGET_IS_TOPMOST: bool = false;

/// Applies the unified tint through a `DesktopAcrylicController` attached to
/// the window's desktop composition target.
pub(crate) fn apply_controller(
    window: &Window,
    style: &WindowBackdropStyle,
) -> Result<AcrylicState, WasdkError> {
    let runtime = RUNTIME.get().ok_or(WasdkError::NotInitialized)?;

    let hwnd = hwnd_of(window)?;

    // Win32 hosts must opt into the host backdrop brush before attaching a
    // system backdrop controller, or DWM may not composite the material.
    const DWMWA_USE_HOSTBACKDROPBRUSH: u32 = 17;
    #[link(name = "dwmapi")]
    unsafe extern "system" {
        fn DwmSetWindowAttribute(
            hwnd: isize,
            attribute: u32,
            value: *const core::ffi::c_void,
            size: u32,
        ) -> i32;
    }
    let host_backdrop: i32 = 1; // TRUE
    let brush_result = unsafe {
        DwmSetWindowAttribute(
            hwnd.0 as isize,
            DWMWA_USE_HOSTBACKDROPBRUSH,
            (&host_backdrop as *const i32).cast(),
            std::mem::size_of::<i32>() as u32,
        )
    };
    if brush_result != 0 {
        return Err(WasdkError::HostBackdropBrush(brush_result));
    }

    let compositor = windows::UI::Composition::Compositor::new()?;
    let interop: windows::Win32::System::WinRT::Composition::ICompositorDesktopInterop =
        compositor.cast()?;
    let target = unsafe { interop.CreateDesktopWindowTarget(hwnd, BACKDROP_TARGET_IS_TOPMOST) }?;
    let container = compositor.CreateContainerVisual()?;
    target.SetRoot(&container)?;
    let controller = generated::DesktopAcrylicController::new()?;
    let configuration = generated::SystemBackdropConfiguration::new()?;

    let to_byte = |channel: f32| (channel.clamp(0.0, 1.0) * 255.0).round() as u8;
    let tint = windows::UI::Color {
        A: 0xFF,
        R: to_byte(style.tint_rgb[0]),
        G: to_byte(style.tint_rgb[1]),
        B: to_byte(style.tint_rgb[2]),
    };
    let fallback = windows::UI::Color {
        A: 0xFF,
        R: to_byte(style.fallback[0]),
        G: to_byte(style.fallback[1]),
        B: to_byte(style.fallback[2]),
    };

    controller.SetTintColor(tint)?;
    controller.SetTintOpacity(style.tint_opacity.clamp(0.0, 1.0))?;
    controller.SetLuminosityOpacity(style.luminosity_opacity.clamp(0.0, 1.0))?;
    controller.SetFallbackColor(fallback)?;
    controller.SetSystemBackdropConfiguration(&configuration)?;
    configuration.SetIsInputActive(true)?;

    let window_id = runtime.window_id_interop.window_id(hwnd)?;
    if !controller.SetTargetWithWindowId(window_id, &target)? {
        return Err(WasdkError::TargetRejected);
    }

    Ok(AcrylicState {
        _controller: controller,
        _target: target,
        _container: container,
    })
}

/// Loads the framework-dependent bootstrap DLL and pins the App Runtime.
///
/// The library handle is intentionally never freed: the runtime must stay
/// loaded for the process lifetime.
fn initialize_bootstrap() -> bool {
    /// `PACKAGE_VERSION` as passed by value to `MddBootstrapInitialize2`.
    #[repr(C)]
    struct PackageVersion {
        revision: u16,
        build: u16,
        minor: u16,
        major: u16,
    }

    type MddBootstrapInitialize2Fn =
        unsafe extern "system" fn(u32, *const u16, PackageVersion, u32) -> i32;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn LoadLibraryExW(name: *const u16, file: isize, flags: u32) -> isize;
        fn GetProcAddress(module: isize, proc_name: *const std::ffi::c_char) -> *const ();
    }

    // Restrict the search to the application directory and System32 so a
    // same-named DLL elsewhere on the search path cannot be loaded instead.
    const LOAD_LIBRARY_SEARCH_APPLICATION_DIR: u32 = 0x0000_0200;
    const LOAD_LIBRARY_SEARCH_SYSTEM32: u32 = 0x0000_0800;

    let name = [
        b'M' as u16,
        b'i' as u16,
        b'c' as u16,
        b'r' as u16,
        b'o' as u16,
        b's' as u16,
        b'o' as u16,
        b'f' as u16,
        b't' as u16,
        b'.' as u16,
        b'W' as u16,
        b'i' as u16,
        b'n' as u16,
        b'd' as u16,
        b'o' as u16,
        b'w' as u16,
        b's' as u16,
        b'.' as u16,
        b'A' as u16,
        b'p' as u16,
        b'p' as u16,
        b'R' as u16,
        b'u' as u16,
        b'n' as u16,
        b't' as u16,
        b'i' as u16,
        b'm' as u16,
        b'e' as u16,
        b'.' as u16,
        b'B' as u16,
        b'o' as u16,
        b'o' as u16,
        b't' as u16,
        b's' as u16,
        b't' as u16,
        b'r' as u16,
        b'a' as u16,
        b'p' as u16,
        b'.' as u16,
        b'd' as u16,
        b'l' as u16,
        b'l' as u16,
        0,
    ];
    let module = unsafe {
        LoadLibraryExW(
            name.as_ptr(),
            0,
            LOAD_LIBRARY_SEARCH_APPLICATION_DIR | LOAD_LIBRARY_SEARCH_SYSTEM32,
        )
    };
    if module == 0 {
        return false;
    }
    let proc = unsafe { GetProcAddress(module, c"MddBootstrapInitialize2".as_ptr()) };
    if proc.is_null() {
        return false;
    }
    let initialize: MddBootstrapInitialize2Fn = unsafe { std::mem::transmute(proc) };
    // Pinned Windows App SDK 1.8; empty version tag; zero minimum version.
    const WINDOWSAPPSDK_RELEASE_MAJORMINOR: u32 = 0x0001_0008;
    let min_version = PackageVersion {
        revision: 0,
        build: 0,
        minor: 0,
        major: 0,
    };
    let status = unsafe {
        initialize(
            WINDOWSAPPSDK_RELEASE_MAJORMINOR,
            std::ptr::null(),
            min_version,
            0,
        )
    };
    status >= 0
}

/// Creates the DispatcherQueue required by WinRT composition on this thread.
fn create_dispatcher_queue() -> windows_core::Result<DispatcherQueueController> {
    let options = DispatcherQueueOptions {
        dwSize: std::mem::size_of::<DispatcherQueueOptions>() as u32,
        threadType: DQTYPE_THREAD_CURRENT,
        apartmentType: DQTAT_COM_NONE,
    };
    unsafe { CreateDispatcherQueueController(options) }
}

fn hwnd_of(window: &Window) -> Result<HWND, WasdkError> {
    let Ok(handle) = window.window_handle() else {
        return Err(WasdkError::NoWindowHandle);
    };
    let RawWindowHandle::Win32(h) = handle.as_raw() else {
        return Err(WasdkError::NonWin32Window);
    };
    Ok(HWND(h.hwnd.get() as *mut core::ffi::c_void))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_configure_lower_backdrop_target_as_non_topmost_when_on_windows() {
        // Arrange
        let expected = false;

        // Act
        let is_topmost = BACKDROP_TARGET_IS_TOPMOST;

        // Assert
        assert_eq!(is_topmost, expected);
    }

    #[test]
    fn should_occupy_distinct_composition_layers_when_stacked_with_renderer() {
        // Arrange
        let backdrop_topmost = BACKDROP_TARGET_IS_TOPMOST;
        let renderer_topmost = harbor_terminal::render::gpu::RENDER_TARGET_IS_TOPMOST;

        // Act
        let is_distinct = backdrop_topmost != renderer_topmost;

        // Assert — ADR 0028: backdrop in lower slot, renderer in upper slot
        assert!(is_distinct);
        assert!(!backdrop_topmost);
        assert!(renderer_topmost);
    }
}

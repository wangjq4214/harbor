//! Platform window chrome: application icon, Win32 titlebar customization, GDI fallback.

use winit::window::Icon;
#[cfg(target_os = "windows")]
use winit::window::Window;

/// Decodes the 256×256 PNG produced by the build script for the native window icon.
pub(crate) fn harbor_window_icon() -> Option<Icon> {
    let image = match image::load_from_memory(include_bytes!(concat!(
        env!("OUT_DIR"),
        "/harbor-window.png"
    ))) {
        Ok(image) => image.into_rgba8(),
        Err(error) => {
            tracing::warn!(%error, "failed to decode bundled application icon");
            return None;
        }
    };
    let (width, height) = image.dimensions();
    match Icon::from_rgba(image.into_raw(), width, height) {
        Ok(icon) => Some(icon),
        Err(error) => {
            tracing::warn!(%error, "failed to create application window icon");
            None
        }
    }
}

/// uxtheme `WTA_NONCLIENT` attribute type for non-client theme options.
#[cfg(target_os = "windows")]
const WTA_NONCLIENT: u32 = 1;
/// Do not draw caption text in the title bar.
#[cfg(target_os = "windows")]
const WTNCA_NODRAWCAPTION: u32 = 0x1;
/// Do not draw the window icon in the title bar.
#[cfg(target_os = "windows")]
const WTNCA_NODRAWICON: u32 = 0x2;

/// Pure packing of undrawn-caption theme flags for `WTA_OPTIONS`.
///
/// Returns `(dwFlags, dwMask)` both set to `NODRAWCAPTION | NODRAWICON`.
#[cfg(target_os = "windows")]
fn caption_nodraw_flags() -> (u32, u32) {
    let flags = WTNCA_NODRAWCAPTION | WTNCA_NODRAWICON;
    (flags, flags)
}

/// Suppresses caption text and the visible caption icon on `window`.
///
/// System min/max/close buttons stay DWM-drawn. Missing HWND or a uxtheme API
/// failure is logged and ignored so startup still proceeds.
#[cfg(target_os = "windows")]
pub(crate) fn suppress_caption_title_and_icon(window: &Window) {
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

    #[repr(C)]
    struct WtaOptions {
        dw_flags: u32,
        dw_mask: u32,
    }

    #[link(name = "uxtheme")]
    unsafe extern "system" {
        fn SetWindowThemeAttribute(
            hwnd: isize,
            e_attribute: u32,
            pv_attribute: *const WtaOptions,
            cb_attribute: u32,
        ) -> i32;
    }

    let Ok(handle) = window.window_handle() else {
        tracing::warn!("caption theme skipped: window handle unavailable");
        return;
    };
    let RawWindowHandle::Win32(h) = handle.as_raw() else {
        tracing::warn!("caption theme skipped: non-Win32 window handle");
        return;
    };

    let (flags, mask) = caption_nodraw_flags();
    let options = WtaOptions {
        dw_flags: flags,
        dw_mask: mask,
    };
    let hwnd = h.hwnd.get();
    let hr = unsafe {
        SetWindowThemeAttribute(
            hwnd,
            WTA_NONCLIENT,
            &options,
            std::mem::size_of::<WtaOptions>() as u32,
        )
    };
    if hr < 0 {
        tracing::warn!(hr, "SetWindowThemeAttribute failed for caption nodraw");
    }
}

/// Paints the opaque backdrop fallback into the window using GDI, before the
/// wgpu surface is ready.
#[cfg(target_os = "windows")]
pub(crate) fn paint_gdi_background(window: &Window, fallback: [f32; 3]) {
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

    #[repr(C)]
    struct Rect {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    }

    unsafe extern "system" {
        fn GetDC(hwnd: isize) -> isize;
        fn ReleaseDC(hwnd: isize, hdc: isize) -> i32;
        fn CreateSolidBrush(color: u32) -> isize;
        fn FillRect(hdc: isize, rect: *const Rect, brush: isize) -> i32;
        fn DeleteObject(obj: isize) -> i32;
    }

    let Ok(handle) = window.window_handle() else {
        return;
    };
    let RawWindowHandle::Win32(h) = handle.as_raw() else {
        return;
    };

    let hwnd = h.hwnd.get();
    let size = window.inner_size();
    let to_byte = |channel: f32| (channel.clamp(0.0, 1.0) * 255.0).round() as u32;
    let color: u32 =
        to_byte(fallback[0]) | (to_byte(fallback[1]) << 8) | (to_byte(fallback[2]) << 16);
    let rect = Rect {
        left: 0,
        top: 0,
        right: size.width as i32,
        bottom: size.height as i32,
    };

    unsafe {
        let hdc = GetDC(hwnd);
        if hdc != 0 {
            let brush = CreateSolidBrush(color);
            FillRect(hdc, &rect, brush);
            ReleaseDC(hwnd, hdc);
            DeleteObject(brush);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_window_icon_decodes() {
        assert!(harbor_window_icon().is_some());
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn should_pack_nodraw_caption_and_icon_flags() {
        // Arrange / Act
        let (flags, mask) = caption_nodraw_flags();

        // Assert — both caption and icon nodraw bits are set for WTA_OPTIONS
        assert_eq!(flags, WTNCA_NODRAWCAPTION | WTNCA_NODRAWICON);
        assert_eq!(flags & WTNCA_NODRAWCAPTION, WTNCA_NODRAWCAPTION);
        assert_eq!(flags & WTNCA_NODRAWICON, WTNCA_NODRAWICON);
        assert_eq!(mask, flags);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn should_return_exact_win32_nodraw_bitmask() {
        // Arrange — documented WTNCA values: NODRAWCAPTION=0x1, NODRAWICON=0x2
        // Act
        let (flags, mask) = caption_nodraw_flags();

        // Assert — ABI-stable packing with no extra bits
        assert_eq!(flags, 0x3);
        assert_eq!(mask, 0x3);
        assert_eq!(flags & !0x3, 0);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn should_return_identical_packing_on_repeated_calls() {
        // Arrange / Act
        let first = caption_nodraw_flags();
        let second = caption_nodraw_flags();

        // Assert
        assert_eq!(first, second);
    }
}

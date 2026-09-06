//! HWND → WindowId interop from the pinned SDK's `Microsoft.UI.Interop.h`.
//!
//! This API is exported by FrameworkUdk, not described by WinRT metadata, so
//! windows-bindgen cannot generate it. The ABI and export name below follow
//! `.tools/wasdk/interactive/include/Microsoft.UI.Interop.h` (SDK 1.8).

use windows::Win32::Foundation::HWND;
use windows_core::{Error, HRESULT, Result, w};

use super::generated::WindowId;

type GetWindowIdFromWindow = unsafe extern "system" fn(HWND, *mut WindowId) -> HRESULT;

#[link(name = "kernel32")]
unsafe extern "system" {
    fn LoadLibraryExW(name: *const u16, file: isize, flags: u32) -> isize;
    fn GetProcAddress(module: isize, name: *const std::ffi::c_char) -> *const ();
    fn FreeLibrary(module: isize) -> i32;
}

/// Owns the module reference backing the SDK's handle-conversion entry point.
/// The process-scoped WASDK runtime retains it after successful initialization.
pub(super) struct WindowIdInterop {
    module: isize,
    convert: GetWindowIdFromWindow,
}

impl WindowIdInterop {
    /// Call only after bootstrap has added the App Runtime to the package graph.
    pub(super) fn load() -> Result<Self> {
        // Includes the package graph, application directory, user DLL directories,
        // and System32, but excludes the current directory and PATH.
        const LOAD_LIBRARY_SEARCH_DEFAULT_DIRS: u32 = 0x0000_1000;
        let module = unsafe {
            LoadLibraryExW(
                w!("Microsoft.Internal.FrameworkUdk.dll").as_ptr(),
                0,
                LOAD_LIBRARY_SEARCH_DEFAULT_DIRS,
            )
        };
        if module == 0 {
            return Err(Error::from_thread());
        }
        let proc = unsafe { GetProcAddress(module, c"Windowing_GetWindowIdFromWindow".as_ptr()) };
        if proc.is_null() {
            let error = Error::from_thread();
            unsafe { FreeLibrary(module) };
            return Err(error);
        }
        // SAFETY: the pinned SDK header specifies this export's exact ABI. The
        // module reference remains owned by this object while the pointer is used.
        let convert = unsafe { std::mem::transmute::<*const (), GetWindowIdFromWindow>(proc) };
        Ok(Self { module, convert })
    }

    pub(super) fn window_id(&self, hwnd: HWND) -> Result<WindowId> {
        let mut id = WindowId::default();
        // SAFETY: the export is valid for self's lifetime; id is a writable ABI
        // WindowId and the caller supplies the live window's native handle.
        unsafe { (self.convert)(hwnd, &mut id) }.ok()?;
        Ok(id)
    }
}

impl Drop for WindowIdInterop {
    fn drop(&mut self) {
        // Balance the loader reference if initialization fails before publication.
        unsafe { FreeLibrary(self.module) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::ManuallyDrop;

    fn with_conversion(convert: GetWindowIdFromWindow) -> ManuallyDrop<WindowIdInterop> {
        // No DLL reference is acquired: never run FreeLibrary on this test fixture.
        ManuallyDrop::new(WindowIdInterop { module: 0, convert })
    }

    unsafe extern "system" fn nonidentity_conversion(hwnd: HWND, id: *mut WindowId) -> HRESULT {
        // SAFETY: window_id supplies a live, writable out-parameter. HWND is only
        // an opaque input token here, never dereferenced or passed to Windows.
        unsafe {
            id.write(WindowId {
                Value: (hwnd.0 as usize as u64) ^ 0xFEDC_BA98_7654_3210,
            })
        };
        HRESULT(0)
    }

    #[test]
    fn should_return_converted_id_when_export_succeeds() {
        // Arrange — include high bits to catch truncation and identity-cast regressions.
        let interop = with_conversion(nonidentity_conversion);
        let hwnd = HWND(0x1234usize as *mut core::ffi::c_void);

        // Act
        let id = interop.window_id(hwnd).expect("successful conversion");

        // Assert
        assert_eq!(id.Value, 0xFEDC_BA98_7654_2024);
    }

    #[test]
    fn should_accept_nonzero_success_when_export_returns_s_false() {
        unsafe extern "system" fn convert(_: HWND, id: *mut WindowId) -> HRESULT {
            // SAFETY: window_id provides the valid output storage.
            unsafe { id.write(WindowId { Value: 42 }) };
            HRESULT(1)
        }
        // Arrange
        let interop = with_conversion(convert);

        // Act
        let result = interop.window_id(HWND::default());

        // Assert — HRESULT success is nonnegative, not only S_OK.
        assert_eq!(result.expect("S_FALSE is a successful HRESULT").Value, 42);
    }

    #[test]
    fn should_propagate_hresult_when_export_fails_after_writing_output() {
        unsafe extern "system" fn convert(_: HWND, id: *mut WindowId) -> HRESULT {
            // SAFETY: window_id provides the valid output storage.
            unsafe { id.write(WindowId { Value: 42 }) };
            HRESULT(0x8007_0057u32 as i32)
        }
        // Arrange
        let interop = with_conversion(convert);

        // Act
        let result = interop.window_id(HWND::default());

        // Assert — failure must not expose the out-parameter as a valid ID.
        assert_eq!(
            result.expect_err("failed HRESULT").code(),
            HRESULT(0x8007_0057u32 as i32)
        );
    }
}

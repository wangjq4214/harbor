//! Load one matching ConPTY API/host pair, independent of the Windows inbox version.

use std::{path::Path, sync::OnceLock};

use anyhow::{Context, ensure};
use libloading::os::windows::{
    LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR, LOAD_LIBRARY_SEARCH_SYSTEM32, Library,
};
use windows::{
    Win32::{
        Foundation::HANDLE,
        System::Console::{COORD, HPCON},
    },
    core::HRESULT,
};

type Create = unsafe extern "system" fn(COORD, HANDLE, HANDLE, u32, *mut HPCON) -> HRESULT;
type Resize = unsafe extern "system" fn(HPCON, COORD) -> HRESULT;
type Close = unsafe extern "system" fn(HPCON);

pub(super) struct ConptyApi {
    pub(super) create: Create,
    pub(super) resize: Resize,
    pub(super) close: Close,
    // The process-wide API outlives every HPCON, including asynchronous close workers.
    _library: Library,
}

impl ConptyApi {
    fn load(directory: &Path) -> anyhow::Result<Self> {
        // The DLL otherwise falls back to inbox conhost when OpenConsole is missing.
        // Require the complete distribution so that a broken package cannot silently
        // reintroduce the old resize repaint behavior.
        for arch in ["x64", "arm64", "x86"] {
            ensure!(
                directory.join(arch).join("OpenConsole.exe").is_file(),
                "missing bundled ConPTY host: {arch}/OpenConsole.exe"
            );
        }
        let path = directory.join("conpty.dll");
        // SAFETY: Only the fixed application-relative DLL is loaded. Its dependencies
        // are resolved in its own directory and System32, never the shell's cwd/PATH.
        let library = unsafe {
            Library::load_with_flags(
                &path,
                LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_SYSTEM32,
            )
        }
        .with_context(|| format!("failed to load {}", path.display()))?;
        tracing::info!(
            path = %path.display(),
            version = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../third_party/conpty/VERSION")).trim(),
            "loaded bundled ConPTY runtime"
        );
        // SAFETY: Signatures match the pinned package's conpty.h (WINAPI/system ABI).
        // All three operations must come from this library, never mix inbox HPCON APIs.
        unsafe {
            Ok(Self {
                create: *library.get::<Create>(b"ConptyCreatePseudoConsole\0")?,
                resize: *library.get::<Resize>(b"ConptyResizePseudoConsole\0")?,
                close: *library.get::<Close>(b"ConptyClosePseudoConsole\0")?,
                _library: library,
            })
        }
    }

    pub(super) fn get() -> anyhow::Result<&'static Self> {
        static API: OnceLock<Result<ConptyApi, String>> = OnceLock::new();
        API.get_or_init(|| {
            (|| {
                let executable = std::env::current_exe()?;
                let directory = executable
                    .parent()
                    .context("executable has no directory")?
                    .join("conpty");
                Self::load(&directory)
            })()
            .map_err(|error: anyhow::Error| {
                format!("{error:#}; distribute the complete conpty directory beside Harbor")
            })
        })
        .as_ref()
        .map_err(|error| anyhow::anyhow!("{error}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_bundle_is_an_error_instead_of_inbox_fallback() {
        let missing =
            std::env::temp_dir().join(format!("harbor-missing-conpty-{}", std::process::id()));
        assert!(!missing.exists());
        let error = ConptyApi::load(&missing)
            .err()
            .expect("missing runtime must fail");
        assert!(error.to_string().contains("missing bundled ConPTY host"));
    }

    #[test]
    fn bundled_api_exports_load() {
        ConptyApi::get().expect("cargo build must stage the complete ConPTY runtime");
    }
}

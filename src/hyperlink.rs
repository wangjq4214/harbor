use anyhow::{Result, bail};

/// Applies Host URI policy before invoking the platform default handler.
pub(crate) fn open(uri: &str) -> Result<()> {
    dispatch(uri, platform_open)
}

fn dispatch(uri: &str, launcher: impl FnOnce(&str) -> Result<()>) -> Result<()> {
    if !allowed_scheme(uri) {
        bail!("hyperlink URI scheme is not allowed");
    }
    launcher(uri)
}

fn allowed_scheme(uri: &str) -> bool {
    let Some((scheme, _)) = uri.split_once(':') else {
        return false;
    };
    if scheme.is_empty()
        || !scheme.as_bytes()[0].is_ascii_alphabetic()
        || !scheme
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'.'))
    {
        return false;
    }
    ["http", "https", "mailto", "file"]
        .iter()
        .any(|allowed| scheme.eq_ignore_ascii_case(allowed))
}

#[cfg(target_os = "windows")]
fn platform_open(uri: &str) -> Result<()> {
    use anyhow::ensure;
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    use windows::core::PCWSTR;

    let wide: Vec<u16> = uri.encode_utf16().chain(std::iter::once(0)).collect();
    let result = unsafe {
        ShellExecuteW(
            None,
            windows::core::w!("open"),
            PCWSTR(wide.as_ptr()),
            None,
            None,
            SW_SHOWNORMAL,
        )
    };
    ensure!(
        result.0 as isize > 32,
        "default URI handler rejected the request"
    );
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn platform_open(_uri: &str) -> Result<()> {
    bail!("opening hyperlinks is unsupported on this platform")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn allowlist_is_ascii_case_insensitive_and_exact() {
        for uri in [
            "http://example.test",
            "HTTPS://example.test",
            "MailTo:user@example.test",
            "FILE:///C:/tmp/a.txt",
        ] {
            assert!(allowed_scheme(uri), "{uri}");
        }
        for uri in [
            "example.test",
            "javascript:alert(1)",
            "httpsx://example.test",
            "1http://example.test",
            "://example.test",
        ] {
            assert!(!allowed_scheme(uri), "{uri}");
        }
    }

    #[test]
    fn rejection_happens_before_launcher_and_launcher_errors_propagate() {
        let called = Cell::new(false);
        assert!(
            dispatch("custom:value", |_| {
                called.set(true);
                Ok(())
            })
            .is_err()
        );
        assert!(!called.get());

        let error = dispatch("https://example.test", |_| bail!("launch failed"))
            .expect_err("launcher failure must be contained as an error");
        assert!(error.to_string().contains("launch failed"));
    }
}

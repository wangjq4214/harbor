use std::path::{Path, PathBuf};

fn resolve(explicit: Option<std::ffi::OsString>, test_exe: &Path) -> PathBuf {
    explicit.map(PathBuf::from).unwrap_or_else(|| {
        test_exe
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("examples/launch_probe.exe")
    })
}

pub(crate) fn launch_probe() -> PathBuf {
    let probe = resolve(
        std::env::var_os("HARBOR_TEST_LAUNCH_PROBE"),
        &std::env::current_exe().unwrap(),
    );
    assert!(
        probe.is_file(),
        "launch_probe not found at {}; set HARBOR_TEST_LAUNCH_PROBE or build with cargo build -p harbor-pty --example launch_probe",
        probe.display()
    );
    probe
}

#[test]
fn explicit_probe_path_overrides_executable_layout() {
    let test_exe = Path::new(r"C:\unrelated cache\deps\harbor.exe");
    let explicit = std::ffi::OsString::from(r"D:\other cache\launch probe.exe");
    assert_eq!(
        resolve(Some(explicit.clone()), test_exe),
        PathBuf::from(explicit)
    );
    assert_eq!(
        resolve(None, test_exe),
        Path::new(r"C:\unrelated cache\examples\launch_probe.exe")
    );
}

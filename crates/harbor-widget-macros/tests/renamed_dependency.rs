use std::path::PathBuf;
use std::process::Command;

#[test]
fn renamed_widget_dependency_resolves_macro_support_paths() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fixture = manifest_dir.join("tests/fixtures/view-macro-renamed/Cargo.toml");
    let target_dir = manifest_dir.join("../../target/view-macro-renamed");
    let status = Command::new(env!("CARGO"))
        .args(["check", "--locked", "--manifest-path"])
        .arg(fixture)
        .env("CARGO_TARGET_DIR", target_dir)
        .status()
        .expect("renamed view-macro fixture must start cargo check");

    assert!(status.success(), "renamed view-macro fixture must compile");
}

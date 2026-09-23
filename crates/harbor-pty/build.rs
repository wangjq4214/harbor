use std::{env, fs, path::Path};

fn copy_if_changed(source: &Path, target: &Path) -> std::io::Result<()> {
    let bytes = fs::read(source)?;
    if fs::read(target).ok().as_deref() != Some(bytes.as_slice()) {
        fs::create_dir_all(target.parent().unwrap())?;
        fs::write(target, bytes)?;
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if env::var("CARGO_CFG_TARGET_OS")?.as_str() != "windows" {
        return Ok(());
    }
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../third_party/conpty");
    println!("cargo:rerun-if-changed={}", source.display());
    let arch = match env::var("CARGO_CFG_TARGET_ARCH")?.as_str() {
        "x86_64" => "x64",
        "aarch64" => "arm64",
        "x86" => "x86",
        other => return Err(format!("no bundled ConPTY runtime for {other}").into()),
    };
    let out_dir = env::var_os("OUT_DIR").ok_or("missing OUT_DIR")?;
    // OUT_DIR is <profile>/build/<package>/out, including custom target directories.
    let profile = Path::new(&out_dir)
        .ancestors()
        .nth(3)
        .ok_or("invalid OUT_DIR")?;
    for executable_dir in [profile.to_path_buf(), profile.join("deps")] {
        let runtime = executable_dir.join("conpty");
        copy_if_changed(
            &source.join(arch).join("conpty.dll"),
            &runtime.join("conpty.dll"),
        )?;
        // ConPTY chooses the host's native architecture, also for emulated clients.
        for host_arch in ["x64", "arm64", "x86"] {
            copy_if_changed(
                &source.join(host_arch).join("OpenConsole.exe"),
                &runtime.join(host_arch).join("OpenConsole.exe"),
            )?;
        }
        for name in ["LICENSE", "VERSION"] {
            copy_if_changed(&source.join(name), &runtime.join(name))?;
        }
    }
    Ok(())
}

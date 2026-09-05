//! Regenerates vendored Windows App SDK bindings for the Harbor backdrop tiers.
//!
//! Usage (run from `.tools/wasdk/gen`):
//!   cargo run -- --out ../../../src/app/window_backdrop/wasdk/generated.rs
//!
//! Metadata source: pinned NuGet packages downloaded into `.tools/wasdk/`:
//!   Microsoft.WindowsAppSDK.InteractiveExperiences 1.8.260708001
//!   Microsoft.WindowsAppSDK.Foundation 1.8.260803002
//! See `.tools/wasdk/pinned-version.txt` for the pinned SDK version.

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let out_default = "../../../src/app/window_backdrop/wasdk/generated.rs".to_string();

    let mut argv: Vec<&str> = vec![
        "--in",
        "default",
        "--in",
        "../interactive/metadata/10.0.18362.0",
        "--in",
        "../foundation/metadata",
        "--filter",
        "Microsoft.UI.Composition.SystemBackdrops",
        "--filter",
        "Microsoft.UI.Composition.ICompositionSupportsSystemBackdrop",
        "--filter",
        "Microsoft.UI.WindowId",
        "--reference",
        "windows",
        "--flat",
    ];

    let mut out = out_default.clone();
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--out" && i + 1 < args.len() {
            out = args[i + 1].clone();
            i += 2;
            continue;
        }
        i += 1;
    }
    argv.push("--out");
    argv.push(&out);

    let warnings = windows_bindgen::bindgen(argv);
    if !warnings.is_empty() {
        eprintln!("{warnings}");
    }
    println!("wrote {out}");
}

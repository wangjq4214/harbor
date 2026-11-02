//! Synthetic ConPTY launch fixture. Build before running the explicit profile PTY test.
//! Only dedicated HARBOR_PROFILE_* test variables are recorded, never the full environment.
use std::{fs, io};

fn report() {
    let path =
        std::env::var_os("HARBOR_PROFILE_REPORT").expect("explicit synthetic report destination");
    let text = format!(
        "pid:{}\nargs:{:?}\ndirectory:{:?}\nliteral:{:?}\nempty:{:?}\nterm:{:?}\ninherited:{:?}\n",
        std::process::id(),
        std::env::args().skip(1).collect::<Vec<_>>(),
        std::env::current_dir().unwrap(),
        std::env::var_os("HARBOR_PROFILE_LITERAL"),
        std::env::var_os("HARBOR_PROFILE_EMPTY"),
        std::env::var_os("TERM"),
        std::env::var_os("HARBOR_PROFILE_INHERITED"),
    );
    // Atomically publish to avoid readers seeing a truncated intermediate report.
    let temporary = std::path::PathBuf::from(&path).with_extension("tmp");
    fs::write(&temporary, text).unwrap();
    fs::rename(temporary, path).unwrap();
}

fn main() {
    report();
    loop {
        let mut line = String::new();
        if io::stdin().read_line(&mut line).unwrap() == 0 {
            break;
        }
        let line = line.trim();
        if let Some(path) = line.strip_prefix("cd ") {
            std::env::set_current_dir(path).unwrap();
        }
        report();
    }
}

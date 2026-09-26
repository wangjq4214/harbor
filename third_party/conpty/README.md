# Microsoft ConPTY runtime

Unmodified DLL/host pairs from Microsoft's stable
[Microsoft.Windows.Console.ConPTY 1.24.260710001](https://www.nuget.org/packages/Microsoft.Windows.Console.ConPTY/1.24.260710001)
NuGet package. Supported OS baseline: Windows 10 build 17763 or newer, per the package.
Source and MIT license: [microsoft/terminal](https://github.com/microsoft/terminal).

- Package SHA256: `175640566A3B59C4B132070EE96C2C77E5AB7EDD2E92732A5EB3610BBF63D90E`
- Per-file hashes are generated in `SHA256SUMS` by the fetch script.
- ABI declarations are copied from the same package to `conpty.h`.
- License to retain in distributions: [LICENSE](LICENSE).

Run `./scripts/fetch_conpty.ps1` after cloning to restore the ignored runtime
binaries from the hash-checked package; `-PackagePath <path>` uses an already
downloaded package. Subsequent Cargo builds are offline and use these local
files. To update, change both the version
and package hash in that script and rerun the Windows PTY/resize tests.

## Runtime layout

`harbor-pty/build.rs` stages `conpty/` beside Cargo executables and test binaries:

```text
harbor.exe
conpty/
  conpty.dll            # matches Harbor's target architecture
  x64/OpenConsole.exe
  arm64/OpenConsole.exe
  x86/OpenConsole.exe
  LICENSE
  VERSION
```

Ship this entire directory with `harbor.exe`. The DLL chooses OpenConsole for
the native host architecture, including when Harbor runs under emulation.
The runtime is loaded only from this executable-relative directory, and the
same loaded library owns create, resize and close for each HPCON. An incomplete
bundle is an explicit startup error; it must not silently fall back to inbox
ConPTY, whose old resize repaints overwrite locally reflowed history.

PTY bytes, including application clear/home/cursor sequences and ConPTY cursor
position queries, are passed intact to the terminal parser. There is no ANSI
redraw suppression, timeout, redraw-count heuristic, or undocumented `0x2` flag.

## Verification

```powershell
cargo test -p harbor-pty
cargo test -p harbor-terminal
```

`live_conpty_rapid_resize_preserves_history_and_prompt` uses a real cmd session,
prints 20 numbered lines, resizes through 30/12/50/8/80 columns five times,
compares all retained text and the prompt cursor, then checks new output. It
failed on Windows 22621 inbox ConPTY with lost history, and exercises the bundled
runtime without a test-only backend override. Manual mouse-driven GUI resizing,
and native ARM64/x86 execution, remain separate verification scenarios.

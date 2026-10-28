# Development Scripts

Scripts are grouped by purpose, not by language or platform. Commands below run
from the repository root unless noted otherwise. Automated script tests live in
`tests/`; manual terminal rendering samples live in `terminal/manual_rendering.*`.

| Directory | Purpose | Entry points |
| --- | --- | --- |
| [`setup/`](setup/) | Restore pinned Windows dependencies | `fetch_conpty.ps1`, `fetch_wasdk.ps1` |
| [`packaging/`](packaging/) | Build Windows release assets | `package_windows.ps1`; `build_windows_msi.ps1` consumes an existing payload; `windows_package_version.ps1` is a shared helper |
| [`docs/`](docs/) | Check Markdown language/links/fragments and compile/run public-guide Rust examples | `check_docs.py`, `check_examples.py` |
| [`profiling/`](profiling/) | Analyze DHAT allocation reports | `dhat_analyze.py`, `dhat_drill.py` |
| [`terminal/`](terminal/) | Interactive terminal probes and synthetic workloads | See the fixture list below |
| [`tests/`](tests/) | Automated tests of the scripts themselves | Python unit tests, `test_sample_process.ps1` and `test_package_windows.ps1` |

## Common Commands

```powershell
# Restore the bundled ConPTY runtime before building on Windows.
./scripts/setup/fetch_conpty.ps1

# Restore WASDK metadata only when regenerating Windows bindings.
./scripts/setup/fetch_wasdk.ps1

# Build and validate both MSI and portable ZIP assets.
./scripts/packaging/package_windows.ps1

# Check documentation and compile/run public-guide Rust examples (not release acceptance).
python scripts/docs/check_docs.py
python scripts/docs/check_examples.py

# Analyze a DHAT report in the current working directory.
python scripts/profiling/dhat_analyze.py dhat-heap.json
python scripts/profiling/dhat_drill.py dhat-heap.json render_frame

# Test scripts without launching Harbor or accessing the clipboard.
python -B -m unittest discover -s scripts/tests -v
pwsh -NoProfile -File scripts/tests/test_sample_process.ps1
powershell.exe -NoProfile -File scripts/tests/test_package_windows.ps1
powershell.exe -NoProfile -STA -File scripts/terminal/test_osc52_manual.ps1 -SelfTest
```

Packaging prerequisites and release-payload details are described in the
[release guide](../docs/app/releases.md). Read each packaging script's help for
optional real MSI/ZIP test arguments. See the [developer profiling guide](../docs/developer/profiling.md) for capture procedures, or use `just dhat-run` to capture a DHAT report.

The documentation checker covers tracked and nonignored untracked Markdown, including local links and heading fragments. It does not validate external sites or factual claims.

The example checker generates an isolated package/cache under `target/` and runs public widget/terminal Rustdoc examples; native `no_run` snippets compile only.

For Rust/resource test categories and CI entry points, see the [testing guide](../docs/developer/testing.md).

## Terminal Fixtures

These are **not a batch test suite**. Most run inside a disposable Harbor tab;
some reset or erase terminal contents, change input modes, launch windows, or
require explicit consent to replace the clipboard. Read each script's parameters
and safety notes before running it.
A fixture completing does not establish visual or end-to-end acceptance.

| Area | Scripts / guidance |
| --- | --- |
| Manual rendering | `terminal/manual_rendering.sh`, `terminal/manual_rendering.ps1`, `terminal/manual_rendering.cmd`; visual inspection only, not automatic E2E acceptance |
| Capability negotiation | `terminal/verify_capabilities.py` (POSIX tty in WSL/SSH); live ConPTY host and usage below |
| Resize and reflow | `terminal/resize_reflow_workload.ps1` |
| Unicode and emoji | `terminal/verify_unicode_windows.ps1`, `terminal/verify_color_emoji_windows.ps1` |
| SGR decorations | `terminal/verify_underlines_windows.ps1`, `terminal/verify_conceal_overline_windows.ps1`, `terminal/verify_modern_sgr_windows.ps1`, `terminal/verify_modern_sgr_neovim.lua` |
| Mouse reporting | `terminal/verify_mouse_windows.py` |
| OSC palette | `terminal/verify_osc_palette_windows.ps1` |
| OSC 52 clipboard | `terminal/test_osc52_manual.ps1` (manual-assisted), `terminal/verify_osc52_windows.ps1` (GUI automation); both require clipboard-replacement consent for runtime checks |
| Terminal search | `terminal/verify_terminal_search_windows.ps1` |

## Capability Negotiation Across ConPTY, WSL and SSH

[`verify_capabilities.py`](terminal/verify_capabilities.py) requires a POSIX tty (WSL or a remote system), Python 3 with curses, and the current TERM's terminfo entry. It records exact expected/observed query bytes, environment/terminfo, per-case PASS/FAIL/NOT RUN and optional cursor/Home/End input. `--vim` runs a clean Vim process, checks its actual key decoder and resulting cursor movement, and does not force escape-key mappings. Use only a disposable tab: the probe changes cursor/mode state. Vim/Neovim visual rendering, physical key routing and SSH are not established by a passing byte probe.

The [GPU-free live host](../crates/harbor-terminal/examples/capability_probe_host.rs) launches a **real Windows ConPTY** session and supplies structured `TerminalEvent` keys at the probe's readiness markers. It uses the normal reply-writing path and has a 300-second deadline. Restore the pinned ConPTY runtime before building; the PTY build script stages it beside example executables too.

PowerShell, from the repository root (adjust distribution and guest repository path):

```powershell
$guestRepo = '/mnt/d/workspaces/harbor'
$probe = "$guestRepo/scripts/terminal/verify_capabilities.py"
$reports = "$guestRepo/target/capabilities"

# Outer Harbor identity, exact query replies and normal/application-mode keys.
cargo run -p harbor-terminal --example capability_probe_host -- wsl.exe -d Ubuntu --exec python3 $probe --path Windows-ConPTY-WSL-Ubuntu --keys --vim --output "$reports/wsl.json"

# Isolated tmux server with no user configuration; do not override its inner TERM.
$workload = "python3 '$probe' --layer tmux --path Windows-ConPTY-WSL-Ubuntu-tmux-clean --keys --vim --output '$reports/tmux.json'"
cargo run -p harbor-terminal --example capability_probe_host -- wsl.exe -d Ubuntu --exec tmux -L "harbor-capabilities-$PID" -f /dev/null new-session $workload
```

A Git Bash invocation additionally needs `MSYS_NO_PATHCONV=1` to preserve `/mnt/...` and `/dev/null` arguments. Use a fresh tmux socket name for each run; the isolated server exits when its sole workload exits. If the host is interrupted before that point, explicitly stop only that named server with `tmux -L NAME kill-server` in the guest. Do not use a global tmux kill command.

The `harbor` layer checks Harbor's exact bytes; the `tmux` layer checks DSR/CPR and keys against **inner** terminfo. It captures other replies/timeouts for diagnosis without asserting that tmux must return Harbor's DA/TN or forward DCS. Such unasserted observations are NOT RUN, not capability passes. The report's overall PASS means its asserted subset passed, not that NOT RUN cases were accepted. Compare the two JSON reports; an inner Home/End of `CSI 1~/4~` is valid when declared by tmux terminfo.

For SSH, first authorize/provision a specific endpoint and place the probe there through your normal trusted process. Use a real allocated remote tty (`ssh -t`), observe TERM/locale/terminfo there, and run the same probe with a named full transport chain and explicit output path. `--keys --timeout 5` allows manual key input; automated `--vim` needs the live host's readiness-key injection. Noninteractive automated SSH should use `BatchMode=yes` and a bounded `ConnectTimeout`, with known host keys/authentication already configured. No fixture installs packages, copies credentials, changes remote configuration, or opens an SSH connection itself. Repeat with `--layer tmux` inside remote tmux. Until actually run, record SSH/tmux-over-SSH as NOT RUN; local tmux is not a substitute.

Generated reports belong under `target/capabilities/` or another explicitly selected output directory, not tracked documentation. Retain them with the source revision/build, bundled ConPTY version and complete transport chain when handing off evidence. The old validation report/checklist paths and checklist-summary script were retired; current documentation checking is `python scripts/docs/check_docs.py`.

## Adding Scripts

Use the existing purpose directories. Keep automated script tests in `tests/`
and interactive fixtures in `terminal/`, even when their names start with `test_`
or `verify_`. Resolve repository-owned resources relative to the script location,
not the caller's working directory; keep user-supplied paths' existing semantics.
Write generated reports, caches and packages under `target/` or explicit output
paths, not alongside scripts.

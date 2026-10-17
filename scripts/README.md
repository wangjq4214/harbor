# Development Scripts

Scripts are grouped by purpose, not by language or platform. Commands below run
from the repository root unless noted otherwise. Existing script names and flags
are unchanged; use the categorized paths instead of the former flat paths.

| Directory | Purpose | Entry points |
| --- | --- | --- |
| [`setup/`](setup/) | Restore pinned Windows dependencies | `fetch_conpty.ps1`, `fetch_wasdk.ps1` |
| [`packaging/`](packaging/) | Build Windows release assets | `package_windows.ps1`; `build_windows_msi.ps1` consumes an existing payload; `windows_package_version.ps1` is a shared helper |
| [`docs/`](docs/) | Check Markdown language/links/fragments, compile/run public-guide Rust examples, summarize VT inventory | `check_docs.py`, `check_examples.py`, `checklist_summary.py` |
| [`profiling/`](profiling/) | Analyze DHAT allocation reports | `dhat_analyze.py`, `dhat_drill.py` |
| [`terminal/`](terminal/) | Interactive terminal probes and synthetic workloads | See the fixture list below |
| [`tests/`](tests/) | Automated tests of the scripts themselves | Python unit tests and `test_package_windows.ps1` |

## Common Commands

```powershell
# Restore the bundled ConPTY runtime before building on Windows.
./scripts/setup/fetch_conpty.ps1

# Restore WASDK metadata only when regenerating Windows bindings.
./scripts/setup/fetch_wasdk.ps1

# Build and validate both MSI and portable ZIP assets.
./scripts/packaging/package_windows.ps1

# Documentation checks and protocol inventory (not release acceptance).
python scripts/docs/check_docs.py
python scripts/docs/checklist_summary.py
python scripts/docs/check_examples.py

# Analyze a DHAT report in the current working directory.
python scripts/profiling/dhat_analyze.py dhat-heap.json
python scripts/profiling/dhat_drill.py dhat-heap.json render_frame

# Test scripts without launching Harbor or accessing the clipboard.
python -B -m unittest discover -s scripts/tests -v
powershell.exe -NoProfile -File scripts/tests/test_package_windows.ps1
powershell.exe -NoProfile -STA -File scripts/terminal/test_osc52_manual.ps1 -SelfTest
```

Packaging prerequisites, release-payload checks and optional real MSI/ZIP test
arguments are described in [validation](../docs/validation.md#windows-release-packages).
See the [profiling guide](../docs/performance/profiling-guide.md) for DHAT capture.

The documentation checker includes tracked and nonignored untracked Markdown (including crate guides) and common local heading fragments; it does not validate external sites or factual claims. Ignored local evidence is excluded in a Git checkout, not deleted. The example checker generates an isolated package/cache under `target/` and runs public widget/terminal Rustdoc examples; native `no_run` snippets compile only. See [documentation maintenance](../docs/README.md#maintenance) for scope and limitations.

## Terminal Fixtures

These are **not a batch test suite**. Most run inside a disposable Harbor tab;
some reset or erase terminal contents, change input modes, launch windows, or
require explicit consent to replace the clipboard. Read each script's parameters
and the relevant [validation guidance](../docs/validation.md) before running it.
A fixture completing does not establish visual or end-to-end acceptance.

| Area | Scripts / guidance |
| --- | --- |
| Resize and reflow | `terminal/resize_reflow_workload.ps1` |
| Unicode and emoji | `terminal/verify_unicode_windows.ps1`, `terminal/verify_color_emoji_windows.ps1` |
| SGR decorations | `terminal/verify_underlines_windows.ps1`, `terminal/verify_conceal_overline_windows.ps1`, `terminal/verify_modern_sgr_windows.ps1`, `terminal/verify_modern_sgr_neovim.lua`; [acceptance guidance](../docs/modern-sgr-acceptance.md) |
| Mouse reporting | `terminal/verify_mouse_windows.py`; [probe guide](../docs/mouse-probe.md) |
| OSC palette | `terminal/verify_osc_palette_windows.ps1`; [acceptance guidance](../docs/osc-palette-acceptance.md) |
| OSC 52 clipboard | `terminal/test_osc52_manual.ps1` (manual-assisted), `terminal/verify_osc52_windows.ps1` (GUI automation); both require clipboard-replacement consent for runtime checks |
| Terminal search | `terminal/verify_terminal_search_windows.ps1`; [search guidance](../docs/terminal-literal-search.md) |

## Adding Scripts

Use the existing purpose directories. Keep automated script tests in `tests/`
and interactive fixtures in `terminal/`, even when their names start with `test_`
or `verify_`. Resolve repository-owned resources relative to the script location,
not the caller's working directory; keep user-supplied paths' existing semantics.
Write generated reports, caches and packages under `target/` or explicit output
paths, not alongside scripts.

# Content-Preserving Resize Automation

## Revision and Scope

- Date: 2026-09-20.
- Source baseline: `05a4c62e34a7fee776704a3a9e74d0a0e9beed78` on `feat/reflow`.
- Tested dirty-tree scope: resize transaction timing in `crates/harbor-terminal/src/lib.rs`, saved-primary history accounting in `crates/harbor-terminal/src/screen.rs`, three mixed/edge-case resize regressions in `crates/harbor-terminal/src/terminal_tests.rs`, and `scripts/resize_reflow_workload.ps1`.
- The final integrated gates below include the completed source, fixture, evidence, and status-document changes.
- Unrelated untracked file excluded from all claims: `TERMINAL_PLUGIN_ARCHITECTURE.md`.
- Scratch command logs were retained locally under `.tmp-t0007/` during this run and are not product evidence.

## Environment

- Windows build: `10.0.26200.9457`.
- Rust: `rustc 1.97.1 (8bab26f4f 2026-07-14)`.
- Cargo: `cargo 1.97.1 (c980f4866 2026-06-30)`.
- Python: `3.12.10`.
- Windows PowerShell: `5.1.26100.9444`.
- Neovim installed: `NVIM v0.12.5`; no interactive Neovim session is claimed by this record.

## Coverage Map

| Acceptance area | Executable evidence | Result |
| --- | --- | --- |
| Primary width/height reflow and round trip | Focused `resize` and `reflow` filters; `repeated_mixed_resize_preserves_selection_and_primary_across_alt_and_retry` | PASS |
| Canonical selection and copy preservation | Focused `selection` filter plus the integrated repeated-resize regression | PASS |
| Capacity, review fallback, cursor meaning, and partial-line eviction | Focused terminal tests and `mixed_resize_keeps_review_content_and_cursor_meaning_for_short_lines` | PASS |
| Alternate rectangular resize and saved-primary restoration | Focused `alt_screen` filter plus the integrated repeated-resize regression | PASS |
| Preparation failure and PTY failure/retry | Existing transaction tests plus injected failure/retry in the integrated regression | PASS |
| Windows ConPTY endpoint resize | `cargo test -p harbor-pty`; interactive ConPTY behavior remains a separate runtime record | PASS (automated scope) |
| Deterministic Windows fixture | Windows PowerShell 5.1 smoke with UTF-8 CJK marker and begin/end markers | PASS |

The integrated regressions retain a selection through width-only, height-only, combined, and round-trip changes with output between commits; preserve CJK hard breaks, trailing blanks, explicit blank lines, review content, and short-line cursor meaning; verify PTY failure rollback; expose saved-primary history accounting while alternate screen is active; and restore the independently resized primary after `?1049` alternate-screen use.

## Focused Checks

All commands ran from the repository root on 2026-09-20.

| Command | Observed result | Outcome |
| --- | --- | --- |
| `cargo test -p harbor-terminal resize` | 35 matching unit tests passed; no failures | PASS |
| `cargo test -p harbor-terminal reflow` | 16 matching unit tests passed; no failures | PASS |
| `cargo test -p harbor-terminal selection` | 72 matching unit tests passed; no failures | PASS |
| `cargo test -p harbor-terminal alt_screen` | 45 matching unit tests passed; no failures | PASS |
| `cargo test -p harbor-pty` | 13 unit tests passed; no failures | PASS |
| `powershell.exe -NoProfile -ExecutionPolicy Bypass -File scripts/resize_reflow_workload.ps1 -Lines 1100 -PayloadWidth 96 -Seed 152170` | Exit 0; 1,100 fixed-width data lines, CJK edge pairs, begin/end markers, and 22 consecutive blank-line pairs verified | PASS |

Fixture SHA-256 at canonical execution: `c554d9517df4c311ec5c5c5234c895d7e1fb8150c28114e818cbfe5d27346f35`.

## Standard Gates

| Command | Started / finished (UTC+08:00) | Observed result | Outcome |
| --- | --- | --- | --- |
| `cargo fmt --check` | 17:08:57 / 17:08:57 | Exit 0 | PASS |
| `cargo clippy --all-targets --all-features -- -D warnings` | 17:09:05 / 17:09:07 | Exit 0 | PASS |
| `cargo test --workspace` | 17:09:07 / 17:09:44 | Exit 0; all executed workspace tests passed; one interactive native-host test and one parser doctest remained ignored | PASS |
| `python scripts/check_docs.py` | 17:12:47 / 17:12:48 | Language and local-file link checks passed after final status-document updates | PASS |
| `python scripts/checklist_summary.py` | 17:12:48 / 17:12:48 | Exit 0; inventory was 799 done, 199 open, 998 total | PASS |
| `git diff --check` | 17:12:48 / 17:12:48 | Exit 0 after final status-document updates | PASS |

## Expected and Observed Result

- Expected: focused resize/anchor/selection/copy/capacity/alternate/failure coverage and every standard repository gate pass against the identified source state.
- Observed: every executed focused and standard command exited successfully. The new integrated regression executed in the focused filters and workspace suite.
- Outcome: **PASS for automated acceptance**.

## Artifacts, Exclusions, and Follow-Up

- Small command summaries are preserved in this record; local scratch logs are not committed.
- This record does not establish interactive PowerShell/cmd, clipboard, Neovim, ConPTY application, visual redraw, latency, or memory acceptance.
- See [Windows runtime evidence](content-preserving-resize-windows.md) and [performance evidence](content-preserving-resize-performance.md) for those independent gates.
- Documentation checks must be rerun after all evidence/status files reach their final form; later source changes require the affected code gates to be rerun.

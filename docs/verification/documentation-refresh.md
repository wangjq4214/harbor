# Documentation Refresh Verification

## Scope and Environment

- Source baseline: `a53395d`; the working-tree change is documentation only.
- Purpose: distinguish current implementation, remaining product work, and missing execution evidence; preserve historical ADR/spec/ticket and memory-capture records.
- Environment: Windows 11 build `10.0.26200`, Rust `1.97.1`, Cargo `1.97.1`, Python `3.12.10`.
- This is a documentation/source audit with the focused executions below, **not** a Windows interactive compatibility or release report.

## Executed Checks

| Command                                                                                                                                                     | Outcome | Observation                                                                                                                    |
| ----------------------------------------------------------------------------------------------------------------------------------------------------------- | ------- | ------------------------------------------------------------------------------------------------------------------------------ |
| `python scripts/check_docs.py`                                                                                                                              | PASS    | English-language policy and local file targets pass                                                                            |
| `python scripts/checklist_summary.py`                                                                                                                       | PASS    | Numbered protocol sections remain parseable after summary deduplication; counts are inventory, not feature/release percentages |
| `git diff --check`                                                                                                                                          | PASS    | No whitespace errors in the patch                                                                                              |
| `cargo test -p harbor-parser -p harbor-terminal`                                                                                                            | PASS    | Parser: 6 unit + 17 public-API tests; terminal: 768 unit + 48 boundary tests; no failures; one parser doctest ignored          |
| `cargo test -q -p harbor-widget --test child_construction --test interaction_focus_actions_theme --test scroll_area --test flex_layout --test flex_runtime` | PASS    | 66 integration tests passed; no failures or ignored tests in these suites                                                      |

A separate local Markdown-fragment scan checked 86 heading links across 117 documentation files with no missing targets. This supplements `check_docs.py`, which checks file paths but not fragments.

Counts describe these specific executions at the recorded source baseline. They must not be copied into moving project overviews as current totals.

The parser/terminal run includes stable property tests and existing reply, OSC, screen, mouse, focus and preedit regressions. The widget run checks the foundations formerly described as missing: keyed construction, parent-directed layout, interaction/actions/theme and generic scroll behavior. Neither run establishes all application-level behavior through ConPTY or visually validates every GPU/native-host path.

## Source-Inspection Conclusions

- Windows quality-gate and Linux parser-fuzz workflow files exist; their remote runs were not inspected.
- Terminal replies, OSC metadata/default colors, SGR mouse, focus and IME preedit have implementations and focused tests; they are not wholly missing feature families.
- Resize remains non-reflow; ordinary screen content lacks a complete combining/grapheme model.
- Settings remain startup-only. Widget-library HMR is a different development feature.
- Tabs, command infrastructure, Acrylic and rounded decorations exist; panes, command-palette UI, search, profiles, Kitty protocols and liquid refraction remain planned/investigation-gated.
- Keyed reorder, Flex, desktop interaction/theme wrappers and ScrollArea already exist. General text entry, overlays, virtualization and UI Automation remain separate toolkit work.
- Native host ownership follows `WinitWindowHost`/`SharedGpu`, not application-owned resources borrowed by the adapter.
- Scratch-buffer reuse and dynamic atlas growth remain open candidates. Their historical allocation percentages do not establish current optimization priority.

The [current-status inventory](../current-status.md), [protocol checklist](../protocol/checklist.md), and technical references contain source/test pointers. This was a targeted reconciliation of identified stale claims, not a fresh proof of every pre-existing protocol checkbox or historical ticket status.

## Not Executed or Not Established

- Full-workspace tests, all-feature clippy and formatting gates were not run in this documentation-only refresh.
- No interactive Windows shell/editor/mouse/IME/backdrop/dogfood session was performed.
- No `chcp`, Kitty transport, WSL/SSH/tmux interoperability or graphics acceptance matrix was executed.
- No cargo-fuzz corpus replay/campaign or remote CI run was executed/verified.
- No new throughput, latency, memory, GPU or visual-effect benchmark was captured.
- No installer/package acceptance or native Unix runtime check was performed.

Those gates remain open. The new [product plan](../next-stage-plan.md) records the work and [validation policy](../validation.md) defines the evidence needed to close them.

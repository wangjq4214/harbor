# Current Status

## How to Read This Page

This inventory was reconciled with the documentation and public source at `22fc4be`. It separates implemented behavior from planned work and release evidence. Linked acceptance records retain their own revision and exclusions; this documentation refresh is not a new native-runtime or performance acceptance run.

| Label                  | Meaning                                                                                               |
| ---------------------- | ----------------------------------------------------------------------------------------------------- |
| Implemented            | A concrete implementation exists for the stated scope; follow the linked tests/checklist for coverage |
| Partial                | A useful subset exists, with the missing behavior stated explicitly                                   |
| Not implemented        | The proposed product behavior is not present in the inspected implementation                          |
| Needs runtime evidence | Source or test coverage does not establish target-platform/application acceptance                     |
| Deferred               | Intentionally outside the current delivery scope, not silently complete                               |

Implementation and validation are separate axes. An implemented feature may still need runtime evidence. Detailed VT support belongs to the [Protocol Checklist](protocol/checklist.md); future priority belongs to the [Roadmap](roadmap.md), and full proposed scope to the [Next-Stage Product Plan](next-stage-plan.md).

## Terminal and Input

| Area                          | Current implementation                                                                                                   | Evidence / boundary                                                                                                                                                                                                                                                                                  |
| ----------------------------- | ------------------------------------------------------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Streaming parser              | Implemented incremental UTF-8, CSI and bounded string-family framing, cancellation/discard behavior                      | [Parser source](../crates/harbor-parser/src/), [property tests](../crates/harbor-parser/src/core/property_tests.rs), [fragmentation tests](../crates/harbor-terminal/src/parser/incremental_tests.rs); arbitrary-input runtime evidence remains a separate gate                                      |
| Screen model                  | Implemented SGR/cursor/editing, margins, wide-cell normalization, scrollback and alternate-buffer isolation              | [Screen](../crates/harbor-terminal/src/screen.rs), [screen tests](../crates/harbor-terminal/src/screen/tests.rs); individual requirements are tracked in the checklist                                                                                                                               |
| Modern underlines | Implemented `4:0..5`, legacy `4`/`24`, `21`, independent SGR `58`/`59`, decorated blanks/wide cells, DECRQSS and boolean `Su` | [T0003 acceptance](modern-sgr-acceptance.md): exact protocol/state/reflow/CPU/GPU checks, configured native Windows Neovim and synthetic copy/resize; automatic detection and broader transports not claimed |
| Conceal and overline | Implemented independent `8`/`28` and `53`/`55`, preserved source/copy/width/background, decorated blanks and DECRQSS | [Integrated acceptance](modern-sgr-acceptance.md); prior cmd redraw anomaly fixed by [ADR 0052](../.grimoire/adr/0052-conpty-live-primary-producer-geometry-and-styled-tail-clipping.md), not a current blocker; native IME/cross-monitor DPI exclusions remain explicit |
| Resize | Implemented transactional primary/history reflow, rectangular alternate resize, content-anchor remapping and scoped ConPTY live-tail clipping | [ADR 0052](../.grimoire/adr/0052-conpty-live-primary-producer-geometry-and-styled-tail-clipping.md) and [T0003 native evidence](modern-sgr-acceptance.md): scoped cmd pending-input and configured Neovim narrow/wide passed; broader runtime matrix and measured latency/memory acceptance remain NOT RUN |
| Unicode content               | Implemented: multi-scalar combining marks, line-start cues, variation-selector presentation width, and ZWJ emoji sequences | [Cell model](../crates/harbor-terminal/src/model.rs), [cell writer](../crates/harbor-terminal/src/screen/edit/cell_writer.rs), T0003 runtime evidence; complex shaping/ligatures remain deferred; [spec 0015](../.grimoire/spec/0015-unicode-terminal-text-correctness.md#unresolved-acceptance-and-discrepancy) retains an unresolved no-wrap width discrepancy and broader acceptance gaps |
| Terminal replies              | Implemented DSR/CPR, device attributes, DECRQM, DECRQSS and XTGETTCAP slices                                             | [Dispatch](../crates/harbor-terminal/src/parser/handlers.rs), [status strings](../crates/harbor-terminal/src/parser/status_strings.rs), [capability queries](../crates/harbor-terminal/src/parser/xtgettcap.rs), [PTY reply drain](../crates/harbor-terminal/src/io.rs); not a missing reply channel |
| OSC metadata and links        | Implemented titles, OSC 7 directory metadata, OSC 8 cell-linked hyperlinks, OSC 133 shell markers                        | [Parser modules](../crates/harbor-terminal/src/parser/), [hyperlink registry](../crates/harbor-terminal/src/screen/hyperlink.rs); directory inheritance and command-navigation UI are separate planned work                                                                                          |
| OSC colors | Implemented 10/11/12 default-color set/query, 110/111/112 reset, and bounded session-owned OSC 4/104 indexed palette set/query/reset | [Default-color handler/tests](../crates/harbor-terminal/src/parser/osc_color.rs), [palette tests](../crates/harbor-terminal/src/palette_tests.rs), and [scoped Windows/GPU acceptance](osc-palette-acceptance.md); indexes 0-255 and hexadecimal RGB subset only |
| Keyboard and paste            | Implemented traditional application cursor/keypad, function/editing/modifier encodings, bracketed paste and confirmation | [Input](../crates/harbor-terminal/src/input.rs), [terminal tests](../crates/harbor-terminal/src/terminal_tests.rs); Kitty keyboard is not implemented                                                                                                                                                |
| Mouse | Implemented 1000/1002/1003 tracking with legacy, 1005 UTF-8, 1006 SGR, 1015 urxvt and 1016 pixel encodings, plus horizontal wheel routing | [Scoped implementation/evidence](multi-encoding-mouse-acceptance.md) and [probe guide](mouse-probe.md); X10 mode 9 is unsupported; mandatory native interoperability remains unverified. |
| Focus and synchronized output | Implemented session-owned 1004 focus reporting and 2026 synchronized-output presentation/recovery                        | [Focus reporting](../crates/harbor-terminal/src/screen/focus_reporting.rs), [terminal source/tests](../crates/harbor-terminal/src/); preserve behavior through session and screen transitions                                                                                                        |
| IME | Implemented commit, transient preedit overlay, candidate placement, composition-key suppression and cleanup | [Terminal tests](../crates/harbor-terminal/src/terminal_tests.rs), [layout](../crates/harbor-terminal/src/render/layout.rs), [host tests](../crates/harbor-widget/tests/winit_contracts.rs); terminal preedit and reusable widget text editing have separate state and routing contracts. |
| OSC 52 clipboard writes       | Implemented bounded system-clipboard UTF-8 write/clear subset; startup allow/deny/confirm, active foreground live-session authorization | [Protocol subset](protocol/checklist.md#247-clipboard), [configuration](../README.md#startup-configuration), automated gates, runtime evidence: BLOCKED; reads, other selections and unrecorded remote-transport acceptance remain unsupported/unverified |
| Optional protocols            | Kitty keyboard/graphics, OSC 52 reads/non-system selections are not implemented | Do not infer support from safe APC/OSC framing; consult [the checklist](protocol/checklist.md) for other partial SGR and protocol families |

## Application and UI

| Area                       | Current implementation                                                                                                                     | Evidence / boundary                                                                                                                                                                                                           |
| -------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Windows PTY                | Implemented ConPTY spawn, I/O, resize and owned shutdown path                                                                              | [Windows PTY](../crates/harbor-pty/src/windows.rs), [endpoint tests](../crates/harbor-pty/src/lib.rs); code-page compatibility needs the planned N03 matrix, not a presumed non-UTF-8 host decoder                            |
| Tabs                       | Implemented application-owned terminal tabs and tab-qualified output routing                                                               | [Tab manager](../crates/harbor-app/src/tab_manager.rs), [coordinator](../src/tab_coordinator.rs); one terminal per tab, not a pane workspace                                                                                  |
| Selection and history      | Implemented selection/copy, soft-wrap-aware copying, scrollback controls, scrollbar, and resize anchor remapping                                           | [Selection model](../crates/harbor-terminal/src/selection_model.rs), [terminal tests](../crates/harbor-terminal/src/terminal_tests.rs); history capacity is not a user setting and post-font-reload copying of retained `PS-BEFORE` was verified on Windows (T0001); the broader interactive clipboard matrix remains **NOT RUN** |
| Startup/live settings | Implemented startup fallback and strict manual/default-on watched reload for palette, bindings and terminal-only fonts; child-only shell/environment/directory; restart-required clipboard policy | [Configuration](../crates/harbor-config/src/lib.rs), [example](../config.example.toml), [T0001–T0004](../.grimoire/spec/0024-watched-reload-and-launch-settings.md#delivery-and-outstanding-acceptance). T0001/T0002 have prior user-confirmed acceptance; T0003 has scoped Windows watching evidence. At `0a4057f` plus the T0004 test diff, combined E10, native env/cwd probes, rejection/compensation and standard Rust gates pass (2465 workspace tests passed, 10 ignored). New cmd/PowerShell children, save/replacement/burst recovery, retained nvim rendering and raw consumed-chord bytes were observed on Windows; fresh IME, physical editing/held-key, selection/copy and complete nvim exit/primary-restoration acceptance remain pending. Detailed evidence is deliberately local-only/Git-ignored, not distributed; see [T0004](../.grimoire/spec/0024-watched-reload-and-launch-settings.md#delivery-and-outstanding-acceptance) for delivery limits and pending remote reconciliation. |
| Application commands       | Implemented registry, availability/dispatch and configurable shortcuts                                                                     | [Command module](../crates/harbor-app/src/command.rs); command palette UI is not implemented                                                                                                                                  |
| Widget HMR                 | Implemented optional Windows debug-development root-library reload                                                                         | [HMR integration](../crates/harbor-widget/src/winit/hmr.rs), [README recipe](../README.md#debug-widget-hot-reload-windows); distinct from settings reload                                                                     |
| Rendering and text | Implemented wgpu terminal rendering, damage updates, DirectWrite fallback, bounded color-emoji presentation, procedural box drawing and modern decorations | [Terminal rendering](../crates/harbor-terminal/src/render/), [box-drawing evidence](procedural-box-drawing-evidence.md), [metric lifecycle](decoration-metric-lifecycle-evidence.md); complex-script shaping and current performance acceptance remain separate work. |
| Window appearance | Implemented Acrylic/fallback, rounded decorations, generic gradients, local backdrop blur and optional edge refraction used by the search panel | [Widget architecture](architecture/widget-runtime.md), [search-panel contract](../.grimoire/spec/0029-search-panel-glass-appearance.md); this is application-local sampling, not desktop capture, a general filter framework or accepted full-product glass/theme delivery. |
| Desktop widget foundations | Layout, focus/actions/shortcuts, themes, scroll containers, view macro, Portal/notification presentation and reusable single-line text editing are present | [Widget guide](widget/README.md), [input contract](widget-input.md) and [input verification](widget-input-verification.md); modal overlays, UIA and the broader catalog remain open. |
| Current-session search | Implemented bounded case-sensitive literal matching, current-content highlights, previous/next commands and a reusable editable search field in a single-row panel | [Application guide](app/README.md), [search integration](current-session-search.md), [text-input verification](widget-input-verification.md); native ongoing-output/resize acceptance and large-history cost measurements remain separate gates. No regex or cross-session search. |
| Remaining product workflows | Pane splitting, named profiles, command palette and command-navigation UI are not implemented | [N05–N09](next-stage-plan.md) retain their remaining scope; delivered search is not completion of all search/shell workflows. |
| Native Unix runtime        | **Deferred:** the Unix PTY backend is a stub                                                                                               | [Unix PTY](../crates/harbor-pty/src/unix.rs); running a shell through WSL/SSH on Windows is a different scope                                                                                                                 |

## Windows Distribution

| Area | Current implementation | Evidence / boundary |
| ---- | ---------------------- | ------------------- |
| MSI | Implemented x64 static-CRT payload, per-user/per-machine scope, scoped shortcuts and same-scope upgrade identity | [Package builder](../scripts/packaging/package_windows.ps1), [MSI builder](../scripts/packaging/build_windows_msi.ps1), [WiX source](../installer/windows/Harbor.wxs). Real MSI construction and headless database/costing checks passed locally; ordinary-user installation, authorized UAC, upgrade and uninstall acceptance remain NOT RUN for this revision. |
| Portable ZIP | Implemented executable-adjacent marker/configuration, independent of launch directory; startup, watching, manual reload and diagnostics reuse one startup-selected path | [Configuration resolver](../crates/harbor-config/src/lib.rs), [application integration](../src/shell.rs), [subprocess regression](../src/configuration_notice_tests.rs). Missing startup configuration silently uses defaults; malformed TOML or unreadable startup files diagnose and use defaults. Rejected reloads retain accepted settings; portable mode never substitutes home settings. Restart to change marker mode. |
| Release automation | Implemented shared PR/release quality and package gates, bounded stable-version validation and separate release publication | [Quality workflow](../.github/workflows/windows-quality-gates.yml), [release workflow](../.github/workflows/windows-release.yml), [package tests](../scripts/tests/test_package_windows.ps1). Workflow lint and local package tests passed; live tag publication and downloaded-asset acceptance remain NOT RUN. Packages are unsigned. |

Local verification covers `2870b8a` plus the portable-path, packaging/MSI,
regression-test and workflow fixes in the working tree. Environment: Windows
build `10.0.26300.0`, bundled ConPTY `1.24.260710001`, PowerShell `7.6.6` and
Windows PowerShell `5.1.26100.9444`, with .NET SDK `8.0.426` for MSI construction.
Format, all-target/all-feature clippy, configuration/application regressions,
static-CRT MSI/ZIP construction and package checks passed. Package checks ran in
both PowerShell versions and include embedded payload, normalized archive paths,
secure `APPLICATIONFOLDER` propagation metadata and late machine-scope costing;
these are not proof of actual elevated installation.

With `HARBOR_TEST_GPU=dx12-warp`, the initial workspace run failed the unchanged
GPU pixel-comparison regression
`render::text::sequence_tests::t0002_native_changed_metrics_replacement_and_failed_preparation_are_coherent`.
Its isolated rerun and a subsequent full `cargo test --locked --workspace` passed
without suppressing or changing the assertion. The intermittent failure remains
unresolved; a successful rerun is not proof it is fixed. Earlier installer/runtime
records are historical, not acceptance of this working tree. See
[Windows Release Packages](validation.md#windows-release-packages) for remaining
lifecycle and live-release checks.

## Verification and Release Readiness

The repository contains focused unit/contract tests and parser property/fuzz infrastructure. That is not equivalent to a recorded Windows daily-use release. See [Validation](validation.md) for exact commands and the evidence-record format.

### Automation Already Configured

- [Windows quality gates](../.github/workflows/windows-quality-gates.yml) run format, all-target/all-feature clippy, locked workspace tests and documentation checks on pull requests to `main`, then build a static-CRT x64 release, construct MSI/ZIP packages and run packaging regressions before uploading checked artifacts.
- [Windows release](../.github/workflows/windows-release.yml) reuses those quality/package gates for stable tags. A separate job with release-write permission uploads both assets to a draft Release and publishes it only after success; PR checks do not publish releases or replace a published Release.
- [Parser fuzz workflow](../.github/workflows/parser-fuzz.yml) configures Ubuntu corpus replay and scheduled/manual bounded campaigns.

These workflow files exist. Their remote run results were not verified in this documentation refresh; do not list CI as missing or claim it is passing solely from configuration.

Still required for a release claim:

- recorded parser fuzz replay/campaign results on a supported host, not only harness/corpus configuration;
- representative Windows application sessions for replies, mouse/focus, IME, resize, shell lifecycle and sustained output;
- `chcp` and Kitty transport probes before asserting compatibility or a Harbor-specific failure;
- comparable performance captures for the current code, including multi-session and visual-effect workloads as they land;
- actionable diagnostics, packaging and dogfood records, with unsupported behavior explicitly listed.

[Historical memory captures](performance/profiling-guide.md#historical-memory-baseline) remain historical. They do not establish current latency, memory, release readiness, or completion of later optimization work.

## Maintaining This Snapshot

When behavior changes, update the relevant row and protocol requirement with source/test evidence. Update the roadmap only when execution priority or milestone status changes. Keep run results in reproducible evidence records rather than turning implementation summaries into unqualified "verified" claims. Preserve historical ADR/spec/ticket context; supersede decisions explicitly instead of rewriting history to match future plans.

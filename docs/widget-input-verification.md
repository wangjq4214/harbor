# Widget Text Input Verification — 2026-10-16

## Scope and Snapshot

- Source: [spec 0028](../.grimoire/spec/0028-reusable-single-line-text-input.md) and [ADR 0065](../.grimoire/adr/0065-layered-widget-text-input.md).
- Base revision: `e3d8238a6c010b192944885a5e5d36913f79ca3c`; uncommitted implementation and documentation changes in this working tree. No commit or release is claimed.
- Changed behavior: reusable controller/core/field, grapheme-safe editing and history, render-advance-based geometry, focused-editor IME/local chords, tagged clipboard completion/native delivery, and replacement of the application search editor. Both workspace and renamed-macro-fixture lock files include the existing `unicode-segmentation` package; the compile-fail diagnostic list now includes `EditableText`.
- Environment: Windows `10.0.26300.0`, x86_64 MSVC; Rust 1.97.1 (`8bab26f4f`), Cargo 1.97.1; PowerShell 7.6.6 in the native smoke window. Native smoke used the application's existing UI font/configuration, not a newly prescribed font or style.
- Route: persisted plan, implementation, test authoring/execution, inline diff review/requirements check, affected fixes, integrated verification and scoped rustfmt. No delegation, external publication or unrelated refactoring.

## Automated Evidence

Commands ran from the repository root after production fixes and formatting:

| Command | Result |
| --- | --- |
| `cargo test --locked --workspace --quiet` | PASS; all executed tests and compile-fail fixtures passed; existing ignored tests remain ignored |
| `cargo test --locked -p harbor-widget --all-features --lib --quiet` | PASS; 561 tests |
| `cargo clippy --locked --all-targets --all-features -- -D warnings` | PASS |
| `cargo fmt --all --check` | PASS |
| `python scripts/check_docs.py` | PASS |
| `python scripts/checklist_summary.py` | PASS; inventory only, not product acceptance |
| `git diff --check` | PASS |

Incremental-cache finalization emitted Windows access-denied notes for existing build-cache directories. Commands still completed successfully; these notes are not converted into product failures or silently claimed absent.

The initial workspace attempts exposed two change-induced fixture mismatches: the renamed dependency's independent lock file lacked the new dependency, and a compile-fail snapshot's alphabetical list of Component implementors changed. Both fixtures were updated without weakening their assertions and the complete workspace command was rerun successfully.

Focused coverage is in:

- `crates/harbor-widget/src/text_editing.rs`: four controller tests, including combining/CJK/supplementary/flag/ZWJ deletion, arbitrary offsets, merging graphemes, history, atomic rejection and transient composition.
- `crates/harbor-widget/src/widgets/editable_text.rs`: variable rendered advances, source/display positions, preedit selection and native hidden-caret semantics.
- `crates/harbor-widget/tests/text_input.rs`: eight non-terminal runtime tests for keyboard/history/notifications, tagged clipboard failures and stale results, focus/remount eligibility, IME confirmation/repeats/loss, pointer capture/scroll/DPI geometry, read-only/disabled modes and local-versus-application shortcut routing.
- `crates/harbor-widget/src/winit/effects.rs`: native-adapter read/write/result/error contract test with injected backends, preserving content-free errors.
- `crates/harbor-app/src/workspace_tests.rs`: existing search integration plus the new resulting-length selection replacement, clipboard command isolation, paste/undo notifications and session-replacement late-result test.

## Windows Native Smoke Subset

A fresh debug executable was built with `cargo build --locked --bin harbor` and launched in its own window (`pwsh — Harbor`). Only the newly launched test window was closed afterward; window disappearance was verified.

Observed sequence:

1. Open search with Ctrl+Shift+F. The decorated field, placeholder and Previous/Next/Close buttons appeared above the terminal.
2. Send ordinary letter key presses into the field. Query text, real caret and search status/highlights changed without executing the terminal's pending command line.
3. Use real foreground Ctrl+A. The field text became visibly selected, rather than selecting terminal text.
4. Copy the selected fixture `c` using real foreground Ctrl+C. Replace it with the literal fixture `v`, then use real foreground Ctrl+V. The visible result was `vc`, confirming native clipboard write/read delivery back to the editor. No unrelated clipboard contents were pasted or logged. The system clipboard was left containing the copied test fixture.
5. Close the test window and verify it no longer exists.

The automation driver's background PostMessage modifier combinations did not represent real Ctrl chords: they inserted ordinary letters. Background WM_CHAR and foreground Unicode-packet text injection also did not establish ordinary typing in this application. Those attempts are transport limitations, not passing shortcut/text checks. Successful shortcut observations above used SendInput after verifying the background failure. Winit letter key events and the native clipboard path supplied the actual field-edit evidence.

Native smoke preceded final formatting and the last preedit-highlight/hidden-caret, controller-replacement cleanup and scale-normalization refinements. It is retained only as evidence for the unaffected Latin/selection/native-clipboard path, not final integrated native acceptance. Raw screenshots remain local; this sequence is the self-contained observation record.

## Requirements Check and Review

| Criteria | Evidence and classification |
| --- | --- |
| R1 / E1 — reusable layers | Match by source and non-terminal runtime tests: controller, undecorated editor and decoration wrapper have no application/terminal business dependencies |
| R2 / E2 — coherent state and notifications | Match by controller/runtime tests; public offsets and programmatic/history policies documented in the API and input guide |
| R3 / E5 — editing, history, clipboard | Match in deterministic failure/eligibility tests; native copy/readback smoke subset observed; extended native interaction remains needs-verification |
| R4 / E3 — Unicode integrity | Match in controller and routed deletion tests; native mixed-font/emoji appearance remains needs-verification |
| R5 / E4 — pointer and geometry | Match in variable-advance, capture, horizontal-scroll and synthetic DPI tests; real resize/DPI/drag visual acceptance remains needs-verification |
| R6 / E6, E10 — IME/focus/routing | Match in event-sequence/focus/local-chord tests; actual Chinese candidate window, native event ordering, late IME across session switches and final DPI positioning remain needs-verification |
| R7 / E7 — presentation/modes | Match by wrapper source and mode tests; ordinary field/selection appearance observed in the native subset |
| R8 / E8–E10 — search migration | Match by workspace/search tests and source removal of SearchEntry; live HMR and full native search/navigation/lifecycle acceptance remain needs-verification |

Inline review inspected the complete tracked diff and new controller, editor, field, test and documentation files. It traced editing requests through EventCtx/router/runtime/native host and search through the existing typed command path. Fixes during implementation covered generic IME dispatch before the external-draw requirement, atomic deletion, old-controller composition cleanup, runtime-issued clipboard identities, preedit selection/hidden caret, render-scale normalization and fixture coherence. No confirmed blocking defect remains in the inspected automated scope. Missing native evidence is not represented as a passing check or a demonstrated product defect.

## Remaining Acceptance

Overall outcome: **unverified for full native acceptance**, not clean/fully accepted.

NOT RUN on the final integrated binary: Chinese IME candidate-window interaction and cancellation/commit ordering, mixed Unicode visual/caret checks, real resize/DPI/drag scenarios, supported live DLL/HMR root replacement, and the extended terminal coexistence matrix. Automated HMR lifecycle tests and headless geometry are not substitutes. No new numeric performance threshold or performance improvement is claimed.

ADR 0065 is `Testing`, not `Completed`; the spec's Draft status is not promoted without all required evidence. The implementation plan is saved locally as `.grimoire/plans/0037-reusable-single-line-text-input.md`; that directory is ignored by existing repository policy, which was not changed. The original spec-only refinement was followed by explicit loop authorization for implementation; no ticket decomposition was created.

The detailed local record also exists under `docs/verification/2026-10-16-widget-text-input.md`. Existing `.gitignore` excludes that directory, so this self-contained summary is provided outside it for version control. No ignore rule was changed and no ignored file was force-added.

See the [input guide](./widget-input.md) for public use and the [validation policy](./validation.md) for the remaining runtime evidence format.

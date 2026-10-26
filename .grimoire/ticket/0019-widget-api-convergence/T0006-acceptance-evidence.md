# T0006 integrated acceptance evidence

## Verdict and execution contract

**Current result: automated gates pass; the user reports native focus/IME and real HMR smoke checks pass. Old-generation release-before-DLL-unload evidence remains open.**

## Initial acceptance — historical snapshot

The following initial findings and command results are preserved for traceability. The repair follow-up at the end supersedes the documentation/lint failures; it does not close native acceptance.

- Source: T0006, spec 0041 R1–R8/E1–E7, ADR 0087, and the full section-7 mapping in UI_API_REVIEW.md.
- Snapshot: `d29120f78c081dfcea2276ca8b080bab33a90abf`, initially clean, plus the new `crates/harbor-widget/tests/public_contract.rs` and this evidence/link. No production code, existing tests, current tutorials, compiler snapshots, or dependency locks were changed.
- Execution date: 2026-10-25; working directory `D:/Code/harbor`; rustc 1.97.1 / cargo 1.97.1; Windows MSVC x86-64.
- Route: existing-source inspection → test execution and one combined public regression → focused inline review/requirements check → final integrated tests/checks → scoped formatting → report. No delegation, commit, new implementation plan, or production/general lint repair. The old 0051 parallel plan is cancelled, not an active implementation authority.
- Planning was omitted because implementation was already integrated and this is a bounded acceptance pass. Historical ticket-local evidence helped identify checks but was not counted as this round's passing evidence.
- Required native focus/IME and real supported HMR DLL generation teardown were not run. They remain open, without substituting mounted tests or HMR compilation.
- Two failed quality checks remain: repository documentation links and strict Clippy. No demonstrated production behavior defect was found in the inspected seams; this is not a claim that every historical diff hunk was independently audited.

## Commands and actual results

Logs are local build artifacts under `target/u4-acceptance/` (not committed). This record retains the commands/results if build artifacts are removed.

| Command / evidence | Result |
| --- | --- |
| `cargo test --offline --workspace --all-features -- --nocapture` | **Pass**, formatted combined tree (before the test-only lint expectation described below): 2,839 passed, 0 failed, 18 ignored. Includes doctests, root Host/HMR tests, widget/macro/application/terminal and dependencies. Log: `workspace-final.log`. Excludes the nested root test subprocess's duplicate one-test summary from totals. |
| Macro UI harness within that command | **51 cases passed**: 13 positive and 38 expected compile failures. The harness counts as one test in the total above. Expected compiler errors/panics in negative/catch-unwind tests are not suite failures. |
| Renamed dependency fixture within that command | **Pass**: separate Cargo fixture resolves `widget_alias`, external SingleChild component, ordinary/final Result nodes, root exports, ExternalSurface providers and macro support paths. |
| `cargo test --offline -p harbor-widget --no-default-features --lib --tests` | **Pass**, 920 passed, 0 failed, 0 ignored; `widget-default.log`. Feature-gated winit test executables contain zero tests here and provide no native/winit behavior evidence. |
| `cargo check --offline --workspace --all-targets --all-features` | **Pass**, refreshed after the new test/formatting; `check-final.log`. Covers examples, tests, app UI DLL and all declared workspace features. Existing test warnings remain. |
| `cargo check --offline -p harbor-terminal --no-default-features --all-targets` | **Pass**; `terminal-no-renderer.log`. |
| `cargo check --offline -p harbor --features widget-hot-reload --release` | **Pass**; `hmr-release.log`. This is compilation, not DLL teardown evidence. |
| `cargo test -p harbor-app --lib workspace::tests::palette_tests::rendered:: -- --ignored --nocapture` | **Pass**, all 3 explicitly ignored glass pixel cases actually executed on AMD Radeon RX 7900 XT / Vulkan; `app-gpu.log`. |
| Same command with `HARBOR_TEST_BACKEND=dx12` and `--offline` | **Pass**, all 3 actually executed on the same adapter / DX12; `app-gpu-dx12.log`. |
| Existing widget GPU tests in final workspace run | **Pass**, including 79 quad-renderer cases, actual material/refraction/clip/external rendering and bridge rendering. No infrastructure-skip notices observed. Adapter-backed execution is distinct from scene/packing assertions. |
| `cargo doc --workspace --all-features --no-deps` | **Pass with 3 warnings**; `rustdoc.log`. Warnings: GlyphAtlas → private AtlasStore, Icon's unescaped `[0,1]`, and backdrop's unresolved caption helper link. Not a warning-free rustdoc gate. |
| Independent tutorial snippet Cargo check | **Pass**, all 9 Rust blocks extracted from `docs/widget/learn.md` (4) and `recipes.md` (5), wrapping blocks without a main in a main. `CARGO_TARGET_DIR=D:/Code/harbor/target cargo check --offline --manifest-path target/u4-acceptance/docs-fixture/Cargo.toml --bins`; `doc-snippets.log`. Native recipe compiled, not launched. Other blocks compiled, not executed as binaries. |
| `python scripts/docs/check_docs.py` | **Fail**, 3 broken local source links; `docs-check.log`; details below. |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | **Fail**, existing warnings/lints promoted to errors; `clippy.log` / refreshed `clippy-final.log`; details below. Not counted as a passing gate. |
| `cargo fmt --all -- --check` | **Pass** on final tree; only the new test was formatted with rustfmt. `fmt-final.log`. |
| `git diff --check` | **Pass**; new test/evidence also inspected explicitly because untracked files are not included in ordinary git diff. |

The first workspace all-feature run also passed (2,838 tests, before the new regression). Final evidence is the later 2,839-test run. The first standalone snippet check hit registry TLS/timeouts and temporarily held Cargo's cache lock, causing two parallel checks to time out without results. Those checks were subsequently run sequentially/offline and passed. A malformed codemode script and a summary-only Python quoting error made no file changes and are not product/test failures.

Final test-only follow-up: strict Clippy on the new test exposed its intentional UI-local Rc capture inside the public Arc callback. Added a narrowly scoped, reasoned `#[expect(clippy::arc_with_non_send_sync)]` on that provider declaration, preserving the capture contract rather than changing thread bounds. The new target then passed `cargo clippy --offline -p harbor-widget --features hmr --test public_contract -- -D warnings`, and the formatted test passed again with both `--features hmr` and `--no-default-features` (`public-contract-final.log`, `public-contract-default-final.log`). Formatting/diff checks passed again. The 2,839-test whole-workspace result remains applicable to unchanged behavior; only this affected test was rerun after the annotation. The refreshed workspace Clippy still fails on the pre-existing acceptance-entry findings.

## Public declaration and section-7 inventory

Paths in this section are relative to `crates/harbor-widget-macros/tests/` unless stated otherwise. Positive fixtures were actually compiled and negative fixtures actually rejected in the final run.

| Target public contract | Current entry / consumer evidence | Rejection / preservation evidence |
| --- | --- | --- |
| ExternalSurface; root/widgets/module imports; on_draw/schedule_with/ime_with | `ui/pass/external_surface.rs`, renamed fixture; real app `terminal_widget`; new mounted mixed declaration | `old_external_surface_names.rs`, `old_external_surface_methods.rs`, `external_surface_overcount.rs`; input/unmount and Arc/lifetime/UI-local captures preserved |
| NotificationCard, NotificationCardStyle, NotificationCardPlacement | `notification_card_contracts.rs`, owning module inputs; mounted card tests and actual app notifications | `old_notification_names.rs`, `old_notification_detail_color.rs`, `notification_position_stretch.rs` |
| ActionHandler, ShortcutMap, CrossAxisAlignment | `composition_root_imports.rs`, `notification_card_contracts.rs`, app workspace/palette/rail | `legacy_composition_types.rs`, notification Stretch rejection; Flex runtime stretching passes |
| Focus::new/child, ShortcutMap::new/child, ActionHandler::new(handler)/child | `composition_root_imports.rs`; mounted child attachment and interaction/focus/action suites | `legacy_composition_methods.rs`, `composition_single_child_blocks.rs`; empty blocks clear and setters replace |
| Button/InteractiveRegion::child_with | root-import fixture, `composable_button.rs`, `local_control_api.rs`, new mixed declaration | old content spelling rejected; mounted content hooks/subscriptions/theme/replacement/clearing tests pass |
| LayoutObserver::from_shared_callback | root fixture; pointer-identity unit test and real application allocation observer | legacy query/method fixtures; mounted layout observer regressions |
| Focus::focus_handle/tab_order/focusable | root fixture; non-focusable wrapper keeps subtree pointer activation | old handle/order/enabled rejected; focusable is not disabled |
| SizedBox::background; TextLabel/Icon/EditableText/TextField::foreground | `appearance_contracts.rs`, mounted appearance/text-input scenes, real app consumers | `old_appearance_entries.rs`; finite-only colors and private raw backgrounds checked; scope of foreground unchanged |
| IconButton::foreground_with; NotificationCard::detail_foreground | appearance/card fixtures and mounted consumption tests | old color_with/detail_color rejected; surface/background not overwritten |
| EditableText/TextField::on_change/on_submit and VerticalDirection | appearance fixture, editor runtime and real palette tests | old callbacks rejected; user-commit-only notification, submit modifiers and IME priority preserved |
| Padding/Align::get_insets; DecoratedBox::get_decoration/get_clip_behavior | root fixture and existing mounted/layout/decoration consumers | `legacy_configuration_queries.rs`; old getters removed, Align getter retained |
| Row/Column/Flex::get_options; IconButton::get_label; SizedBox::get_requested_width/get_requested_height | root/appearance fixtures and widget tests | old getter spellings rejected |
| ScrollController::requested_offset versus ScrollMetrics::offset | root fixture and `requested_offset_waits_for_a_successful_layout_commit` | old controller getter rejected; request normalization and jump/scroll/reveal remain unchanged |
| Root Component/BuildCx/View, Stack/TextLabel and construction/extension traits | root fixture, new out-of-crate mixed test, renamed fixture, tutorial checks | `private_view_constructor.rs`, `private_signal_subscriptions.rs`, KeyChord private-field fixtures; no prelude or public ControlSurface |
| Retained convenience constructors/types/callbacks | workspace compilation, `local_control_api.rs`, `public_runtime_boundaries.rs`, app use and regressions | Button::text/icon, SizedBox::width/height, Separator::horizontal/vertical, ThemeProvider::from_signal, Flex::with_options; MainAxisAlignment/PositionAlignment/Alignment2D/FocusHandle/controllers and callback distinctions preserved |

Negative filenames without a directory above live in `ui/fail/`; positive filenames live in `ui/pass/`. Removed module/type paths were also inspected; no compatibility aliases were found. Historical review/spec terminology is not treated as a current API declaration.

## Requirement-to-evidence mapping

`Match` means the bounded observable contract is established; `needs-verification` is not acceptance. Tests below live under `crates/harbor-widget/tests/` unless qualified.

| Requirement | Result | Current evidence and limits |
| --- | --- | --- |
| R1 full type migration, no aliases | **Match** | Public inventory above, root/module exports and application consumers; old entries fail compilation. |
| R2 constructors, slots, queries | **Match** | `child_attachment`, `single_child_lifecycle`, `composable_button`, `interaction_focus_actions_theme`, `scroll_view`, macro evaluation tests; new `public_contract` combines the final APIs through real mount/layout/scene. |
| R3 properties/callbacks/polarity/timing | **Match for automated scope** | Complete public inventory; `text_input::explicit_vertical_directions_and_text_callbacks_keep_ime_priority_data_and_submit_modifiers`; palette/search/rail consumers, external provider tests; native OS evidence separately absent. |
| R4 raw scalar/color domains versus derived layout | **Match** | `appearance_validation` covers every style state/channel, ring, roles, scalar invalid values/zero/extremes; `notification_card` covers positive max_width, optional colors, placement, default values and valid derived overflow with atomic geometry preservation. |
| R5 direct Result versus dynamic panic | **Match** | Initial and later Signal-theme consumption (including inactive states), dynamic foreground repeated consumption, recoverable setters and final macro field/source-location panic tests. Intermediate-chain auto-unwrap compile failures remain. No Runtime recovery promise inferred from catch_unwind. |
| R6 named material and preserved caps/effects | **Match for tested rendering contracts** | `backdrop_material`, renderer packing tests, unchanged material shader algorithm, app recipe constants, actual GPU material/clip/foreground regressions, all three actual app glass pixel cases on Vulkan and DX12. No subjective pre-migration screenshot A/B is claimed. |
| R7 positioning-only versus Stretch, root imports/private seams | **Match** | PositionAlignment card inputs and rejection fixture; flex_layout/flex_runtime and mounted placement/resize/close matrix; root/private fixtures. |
| R8 repository/business/runtime preservation and evidence boundaries | **Match in executed automated paths; gaps remain** | App palette/search/rail/notification and terminal bridge regressions pass; Store FIFO/no-wake, U1–U3 and layout tests pass. Broken source-doc links and lint gate remain. Native focus/IME and real HMR generation teardown remain needs-verification. No U5/U6 completion inferred. |

## Scenario-to-evidence mapping

| Scenario | Result / evidence |
| --- | --- |
| E1 declaration → real mount | **Pass in automated scope.** New `public_contract.rs` compares hand-built versus macro scenes for static theme Result + ActionHandler/ShortcutMap/Focus + Stack/Flex Stretch + Button style/child_with/style Result + multi-Result card + ExternalSurface. It asserts deferred content, error-free layout, source subscription updates, hook value, root identity, no sibling unmount, final external unmount/registration removal and subscription cleanup. Existing child/macro tests establish replace/clear, key identity, evaluation-once/laziness/children-before-parent/deferred build; renamed fixture establishes hygiene. |
| E2 invalid static entries | **Pass.** Complete appearance/card scalar/color matrices, zero and large finite valid values, finite RGBA outside [0,1], recoverable direct errors and source-located final-node panics. Intermediate Results still need explicit handling. |
| E3 dynamic themes/foreground | **Pass.** Appearance validation and theme/control suites exercise first and later actual consumption, all theme roles/inactive states, field-specific panic and correct theme/foreground ownership. |
| E4 derived overflow | **Pass.** Appearance/card/control-surface and layout suites cover narrow/tight/wide/unbounded constraints, legal scalars overflowing only in combination, initial zero fallback and preservation of previously committed geometry. |
| E5 actual glass | **Pass for automated GPU contracts.** Named input/domain/cap/packing checks plus actual material rendering and real app palette-selection, search-hover and notification material pixel tests at 1×/1.5×/2×, Bgra8Unorm and Bgra8UnormSrgb, on both Vulkan and DX12. The three app tests require an adapter and cannot silently pass by skipping. |
| E6 card positions / Flex Stretch | **Pass.** `placement_close_hit_and_resize_cover_all_positioning_alignments`, default/long-detail/scroll/stable-close tests, app notification lifecycle tests and actual notification pixels; compile rejection of notification Stretch alongside genuine Flex stretching. |
| E7 business/editing/external/scroll | **Pass for mounted/adapter/GPU paths; native acceptance open.** Real palette reduction/generation/IME tests, search command routing, rail nested-close-first-refusal, request→successful scroll commit, `workspace_tests::stable_external_surface_rebuild_preserves_input_ime_and_unmount_cleans_up`, external/winit/scheduler/bridge tests. These do not establish OS focus/candidate-window behavior or real DLL unloading. |

## Findings and documentation gap list

1. **Confirmed documentation regression / acceptance issue (high confidence):** `python scripts/docs/check_docs.py` fails on:
   - `.grimoire/spec/0026-widget-and-application-component-boundaries.md:47` → removed `widgets/notification.rs`.
   - `.grimoire/spec/0039-return-driven-event-consumption-and-commit.md:86` → removed `widgets/custom_paint.rs` and `widgets/notification.rs`.
   Historical terminology may remain, but live source-link targets no longer resolve after module migration. Recommended follow-up: repair targets to external_surface.rs/notification_card.rs without rewriting historical decisions.
2. **Failed strict quality gate, not evidence of a production bug:** Clippy reports unused imports/variables, non-Send/Sync Arc lints, bool clone and a macro-test blocks-in-conditions lint. Examples include `macro_tests.rs`, `layout_component_tests.rs`, `single_child_lifecycle.rs`, and `terminal_view.rs:2088`. They existed on arrival for this acceptance run; no pre-U4 Clippy baseline was executed, so attribution to individual migration commits is not claimed. Do not tighten provider Send/Sync bounds to silence the intentional UI-local capture cases; use appropriate scoped explanations or test-only fixes in a separate authorized cleanup.
3. **Advisory rustdoc issue:** `widgets/icon.rs:34` uses unescaped `[0,1]` and creates an unresolved link warning. Two other rustdoc warnings are recorded above. Cargo doc succeeds but is not warning-clean.
4. **Current tutorial/example evidence:** all nine independent learn/recipes Rust blocks compile, including the native host recipe. Current widget and terminal READMEs and widget catalog/concepts/layout/testing migration text were inspected for old current API entries; no stale selected API spelling was found in that inspected current-doc set. Catalog/concepts explanatory fragments were not compiled independently; native recipes were not exercised interactively. Rustdoc success and snippet compilation do not validate user interaction or visual parity.
5. **Required evidence still missing, not a proven code defect:** OS-native focus/IME and real supported DLL generation teardown. No human/native result was supplied or fabricated.

Review focused on changed exports/wrappers, validation consumers, dynamic theme/foreground, card layout/positioning, material constructors/renderer packing, text callbacks, actual app migration consumers, and provider/terminal bridges. The runtime/layout/event-source changes inspected outside those owners were test migrations or naming changes rather than a new runtime policy. New test and evidence files were inspected in full. This is not an exhaustive independent security/performance review of every test snapshot or historical document across the 145-file migration.

## T0006 acceptance disposition and remaining handoff

- Public mapping/rejection, combined declarations, mounted lifecycle, static/dynamic validation and derived geometry: established by this run.
- Real application E5–E7: established at GPU/mounted/adapter layers, **not complete at native focus/IME/HMR layers**.
- Suites/features/docs/examples/format/diff: actual outcomes recorded; failed docs/Clippy are explicitly not passing.
- Separate GPU/native/HMR traceability and complete R/E mapping: provided here, without closing unfinished obligations.
- Ticket/spec/ADR statuses and acceptance checkboxes are deliberately unchanged. Neither U4 nor old native obligations nor U5/U6 are marked complete.

Next actions: repair the three documentation link targets; resolve or explicitly scope the strict lint failures without changing provider contracts; then collect version-bound native focus/IME and real supported HMR generation teardown evidence. After writes, refresh affected checks and bind native results to that actual snapshot. A genuine type/tree remount must be reported as such; no cross-parent or old/new DLL type-identity preservation is promised.

## Follow-up — documentation and strict Clippy repair

The user authorized repairing the documentation links and Clippy failures after the initial acceptance. Snapshot: the same HEAD plus the earlier acceptance files and this scoped cleanup; no commits or dependency changes.

Changes:
- Repaired the three source-link targets in spec 0026/0039, preserving historical terminology and decision/status text.
- Fixed all three rustdoc warnings: Icon's numeric interval is code, public GlyphAtlas docs describe private AtlasStore without an inaccessible link, and the caption helper link points to its actual chrome module.
- Removed unused imports, named unused test contexts `_cx`, and replaced a bool clone with dereferencing.
- Retained public Arc providers and UI-local Rc captures. Narrow `#[expect]` annotations with explicit reasons document intentional retained-Fiber/provider cases. The macro test's side-effectful scrutinee also has a scoped expectation: hoisting it would weaken its laziness/evaluation coverage. No crate-wide lint suppression or Send/Sync tightening was introduced.
- Production execution, callback timing, ownership and test assertions remain unchanged; executable edits are confined to test code/helpers.

Refreshed checks at `D:/Code/harbor`:
- `cargo clippy --offline --workspace --all-targets --all-features -- -D warnings`: **passed**, `target/u4-acceptance/clippy-repair.log`.
- `python scripts/docs/check_docs.py`: **passed**, 185 Markdown files, including the evidence/link updates.
- `RUSTDOCFLAGS="-D warnings" cargo doc --offline --workspace --all-features --no-deps`: **passed without warnings**, `rustdoc-repair.log`.
- `cargo test --offline --workspace --all-features -- --nocapture`: **2,839 passed, 0 failed, 18 ignored**, `workspace-repair.log`; excludes the nested subprocess's duplicate one-test summary. No GPU infrastructure-skip notices observed.
- `cargo fmt --all -- --check` and `git diff --check`: **passed**.
- Reviewed the complete repair diff; existing acceptance test/evidence work was preserved.

Documentation/lint findings 1–3 above are resolved. Remaining required evidence is still OS-native focus/IME and real supported HMR DLL generation teardown. Ignored tests are not newly counted as executed; the earlier explicit GPU pixel results apply to unchanged rendering code. Ticket/spec/ADR completion statuses and acceptance checkboxes remain unchanged.

## Human feedback — native interaction and real HMR smoke

After receiving the native focus/IME and real Windows debug HMR checklist, the user reported no issues in any of the checks.

- Record native focus/IME, input ownership/switching and the preceding interaction checklist as **passed, user-reported**.
- Record real HMR observable operation/reload/recovery smoke checks as **passed, user-reported**, not independently observed by the agent.
- This feedback follows the documentation/Clippy repair snapshot. HEAD remains `d29120f78c081dfcea2276ca8b080bab33a90abf`; the live worktree shows the same repair/acceptance file set. No separate executable/DLL build identity, exact reload count or runtime trace was supplied, so those details are not invented.
- The previous checklist explicitly distinguished successful reload/no visible faults from proof that old-generation callbacks and registrations are released before DLL unload. The user supplied no logs/probes establishing that ordering; this specific internal teardown obligation remains **needs-verification**, not a demonstrated defect.
- No code changes or automated acceptance substitution were made for this feedback. Ticket/spec/ADR completion statuses remain unchanged; no U5/U6 completion is inferred.

Remaining handoff: obtain a version-bound real-generation lifecycle trace/probe establishing release-before-unload; do not ask the user to repeat the already reported interaction smoke checks merely because internal tracing is missing.

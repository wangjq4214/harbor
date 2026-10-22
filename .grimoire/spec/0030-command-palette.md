# Command Palette: Existing-Command Delivery

**Spec ID:** 0030
**Status:** Draft; approved direction, not implemented or runtime-accepted.
**Date:** 2026-10-18
**Sources:** [Issue #156](https://github.com/wangjq4214/harbor/issues/156); the user's 2026-10-18 approval of the proposed defaults and Spec → Slice route.
**Material extension:** [ADR 0070](../adr/0070-shared-application-glass-presets-with-local-overrides.md) defines shared panel presets and a distinguishable editor; [spec 0032](./0032-native-liquid-glass-material.md) selects the native liquid-glass upgrade without changing this spec's command, editing or focus contracts. These links do not establish runtime acceptance.

## Requirements

### R1 — Bounded scope and truthful discovery

Deliver a searchable palette for existing application commands. The first-delivery set is all currently implemented commands marked `palette_visible` in the inspected registry: configuration reload; tab creation, closing and navigation (including numeric selection); terminal copy/paste and primary-buffer scrolling; current-session search open/close/next/previous. `terminal.copy-or-interrupt` remains palette-hidden. Internal intents and parameterized-only UI operations do not automatically become discoverable entries.

Respect the existing registry rather than maintain an independent catalog of labels, IDs or handlers. Do not present unimplemented profile, pane or appearance actions as working. Those remain explicitly deferred #156 scope, not prerequisites for this delivery. A bounded increment may deliver an explicitly documented subset of R1; that is partial delivery, not completion of this spec or #156.

### R2 — Opening, filtering and current shortcuts

Provide a registered open-palette operation with default Ctrl+Shift+P, configurable through the existing structured keybinding system. Preserve omission/default, replacement, unbinding, conflict detection and atomic startup/live-reload behavior. The stable ID's exact spelling is an implementation choice to be documented with the delivered binding.

Opening focuses the query editor. Filter on display label and stable command ID using case-insensitive substring matching, not fuzzy matching. Empty query exposes the eligible catalog; no matches has a visible empty state and cannot execute a phantom selection. Navigation/selection must remain valid as filtering changes the result set.

Show each command's label and currently effective shortcuts from resolved bindings, including multiple bindings and unbound commands. Do not show superseded defaults after an override or accepted configuration reload. A failed reload retains the previously accepted display and bindings.

### R3 — Keyboard operation and reusable editing

Up/Down select candidate actions; Enter invokes the selected available action once. Escape first cancels active IME composition; outside composition it closes the palette. Confirming an IME candidate must not also invoke an action or navigate results.

Use the existing reusable single-line text editor and its caret, selection, Unicode-safe editing, clipboard and undo/redo behavior. Field editing shortcuts affect the query, not terminal selection or terminal paste. Transient preedit is not a committed filter edit. Preserve [spec 0028](./0028-reusable-single-line-text-input.md), including late clipboard eligibility and focus-loss composition cancellation.

### R4 — Availability and execution feedback

Use shared application-command availability facts for presentation and invocation. Palette-visible actions that cannot currently execute are visibly disabled with a reason. Recheck before execution: a formerly enabled candidate cannot bypass changed context or act on a destroyed target. For example, primary-buffer scroll actions are unavailable in the alternate screen; selecting them must not send Enter or a scroll key to the terminal.

Preserve underlying operation semantics and existing shortcut behavior; do not turn shortcut `PassThrough` into a palette fallback. Empty/no-selection copy still uses the existing always-copy command. No-op/context-dependent behavior must not be represented as a newly successful operation without evidence.

Provide observable execution feedback sufficient to distinguish an executed request, a rejected/unavailable action and an operation failure. Queueing alone is not proof of successful completion. Existing command feedback/notification flows can be reused; do not suppress configuration reload errors or paste confirmation. Exact feedback placement, wording and duration are not standardized by this contract.

### R5 — One command path and existing ownership

The palette is application UI composition. Labels, command meaning, filtering, availability policy, query limits if any, selection and execution feedback remain application-owned. Reuse the registry and typed FIFO command path; the Host remains reducer/executor and owner of terminal, clipboard, window and platform effects. Do not introduce another command bus or a direct palette-to-PTY/terminal handler path.

Reusable text input, Portal, layout, decoration, focus and generic routing remain widget mechanisms without Harbor command/session dependencies. No new crate or mandatory module/type naming is selected.

### R6 — No consumed input leakage

Palette query edits, clipboard edits, candidate navigation, confirmation, cancellation and IME handling must not write their consumed bytes or synthetic markers to the PTY. This includes the opener and the gesture that closes the palette or changes focus; test associated key-up/IME events rather than proving only key-down consumption. An empty result or rejected action must not fall through to the shell.

Preserve terminal shortcut precedence after returning to the terminal, including Ctrl+C without selection and alternate-screen scroll pass-through. Preserve existing cross-window confirmation gates and clipboard authorization; palette invocation must not bypass them.

### R7 — Result-aware focus and IME restoration

Cancel/close returns to the active terminal with its eligible IME behavior restored and no stale palette composition. Execution respects the action's destination instead of unconditionally restoring the old terminal:

- Tab creation/activation targets the resulting active terminal; close follows existing tab/window-close behavior, including the final tab.
- Opening search targets its editor; closing search restores terminal focus. Match navigation retains existing search semantics.
- Paste confirmation follows existing separate-window focus/input gating. It must not leave the palette editor eligible for later clipboard/IME delivery.
- Reload, copy and scrolling return to terminal interaction when no command-specific destination supersedes it.

Context changes, window focus loss and supported root replacement must not cause a closed/replaced editor to receive stale edits or execute a stale selection. Retain established native-host/HMR ownership and teardown contracts; this is not a new persistence/session-restoration feature.

### R8 — Scoped evidence and honest package status

Record focused tests and reproducible Windows end-to-end evidence for the exact delivered revision and command scope. Record dirty-tree scope, OS/ConPTY/application versions, steps, expected versus observed outcomes, retrievable artifacts, exclusions and PASS / FAIL / NOT RUN / BLOCKED. Use synthetic test content, not secrets or unrelated terminal/clipboard payloads.

Update user/developer status documentation only for implemented and evidenced behavior. This first-delivery spec does not complete profiles, panes or appearance commands and does not authorize closing #156 as fully delivered. The issue requires implementation sub-issues when work starts and explicit links for remaining/deferred scope before closure.

## Solution and Integration Seams

The inspected baseline is `28f2a159449fc3f43e0b9e4aa174a5cd721d7d12`. Existing registry labels and visibility are in [command.rs](../../crates/harbor-app/src/command.rs); effective binding publication, root composition and the sole application intent inbox are in [workspace.rs](../../crates/harbor-app/src/workspace.rs). Existing execution is in [shell.rs](../../src/shell.rs); [search.rs](../../crates/harbor-app/src/search.rs) illustrates command-specific focus results. These are integration points, not mandates to put all new logic into those files.

| Seam | Connects | Expects | Provides |
| --- | --- | --- | --- |
| Discovery | Registry/resolved bindings → palette | Implemented visible entries, current binding table and context | Searchable labels/IDs, truthful shortcut display and availability; no duplicate catalog |
| Editing/routing | Widget editor/focus → application palette | Committed edits distinct from preedit; local editing ownership | Query/filter/selection intent; consumed input never becomes terminal input |
| Invocation | Palette → existing typed FIFO → Host | Selected command, current availability and source-aware routing | One execution path, rejection without pass-through, existing operation effects |
| Completion/focus | Host command result → UI/native host | Actual operation outcome and resulting destination | Feedback and correct terminal/search/confirmation focus and IME eligibility |
| Configuration | Accepted reload → bindings publication/palette | Existing atomic acceptance/failure policy | Labels retain current effective shortcuts; rejected settings never leak into display |

Do not conflate editor state, application palette state, command availability and shortcut consumption. Exact signatures and storage are implementation choices, subject to these contracts.

## End-to-End Verification

| ID | Given / when | Required observation |
| --- | --- | --- |
| E1 | Terminal focused; open with default chord; filter with mixed-case label and ID substrings; use Up/Down and Enter for New tab | Query focused, matching candidates shown, exactly one new tab, resulting terminal focused; consumed keys absent from PTY |
| E2 | Replace/unbind/add multiple command bindings; accept reload, then reject a later invalid reload | Palette displays current effective chords/unbound state; old defaults disappear only on acceptance; failed candidate changes neither display nor behavior |
| E3 | Empty query, no-match query and filtering that removes the selected row; press Enter | Eligible catalog/empty state is truthful; selection stays valid; no phantom execution or shell newline |
| E4 | Alternate screen active, then availability changes before invocation | Scroll actions disabled with reasons; stale invocation rechecked; no Enter/scroll bytes injected; ordinary terminal pass-through works after close |
| E5 | Terminal has selection; edit/copy/cut/paste/undo query and exercise Up/Down/Enter/Escape | Field owns editing; no unintended terminal clipboard action or PTY input; Escape closes outside composition and terminal commands resume |
| E6 | Windows Chinese IME composing in query; confirm/cancel candidate, then close palette | Candidate confirmation commits once without action execution; first composition Escape cancels only composition; close restores correct terminal IME/focus; no stale commit reaches terminal |
| E7 | Execute reload, tab navigation/numeric selection/close and ordinary scroll/copy from palette | Existing command outcomes preserved, invocation occurs once, errors/rejections distinguishable, correct active terminal/window-close result |
| E8 | Execute Search open/next/previous/close and Paste with multiline confirmation | Search receives intended focus/status; confirmation gate and later focus policy preserved; closing palette never overrides the destination |
| E9 | Change active context/remove target, lose window focus, close editor with pending clipboard/IME, or replace a supported UI root | No stale action/edit reaches an unrelated target; availability and focus remain coherent with existing lifecycle contracts |

## Decisions and Traceability

- R1 and R5: #156, [ADR 0040](../adr/0040-application-command-dispatch-and-keybinding-precedence.md), [ADR 0029](../adr/0029-widget-store-state-action-boundary.md), [ADR 0064](../adr/0064-widget-and-application-component-ownership.md); existing command scope verified against the inspected registry.
- R2 and R3: user-approved defaults (Ctrl+Shift+P, case-insensitive label/ID substring matching, Up/Down/Enter, composition-first Escape); binding grammar follows [ADR 0041](../adr/0041-structured-keybinding-configuration.md); editor behavior follows [ADR 0065](../adr/0065-layered-widget-text-input.md)/spec 0028.
- R4, R6 and R7: #156 and approved source-aware availability/result-aware focus direction, recorded in [ADR 0069](../adr/0069-command-palette-source-aware-availability-and-focus.md). Native lifecycle follows [ADR 0031](../adr/0031-widget-winit-adapter-owns-native-host-infrastructure.md).
- R8: #156's delivery/evidence rules. This specification is a requirements artifact, not evidence of product behavior.

## Verification Obligations

Add focused registry/configuration, matching/selection, availability/revalidation, widget-routing and Host-integration coverage. Exercise actual typed queue/drain paths with execution counts and a PTY writer capture, not only isolated filter functions. Native Windows E1–E9 evidence remains necessary where keyboard, focus, IME, clipboard and lifecycle are involved; existing editor/search smoke does not automatically validate the palette.

At implementation acceptance run `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test --workspace` and `python scripts/docs/check_docs.py`. #156's older `scripts/check_docs.py` path has moved; [the current developer guide](../../docs/developer/README.md) explicitly says the checklist-statistics script was removed. Record `python scripts/checklist_summary.py` as NOT RUN with that reason rather than recreating it or claiming a pass. Keep durable evidence under `docs/verification/` (create it during evidence work) or link retrievable CI artifacts, as #156 permits.

## Out of Scope and Deferred Package Work

- Implementing profiles, independent panes or product appearance switching. Integrate their real commands as their owners deliver them; retain their #156 tracking rather than adding stubs or speculative implementation tickets here.
- Fuzzy matching, shell-command execution, a second command bus/editor, or changes to underlying search/clipboard/terminal semantics.
- New promises about mouse dismissal, ranking beyond substring filtering, MRU/history, localization, persistence, pixel styling or performance thresholds. These were not selected; existing ownership and usable keyboard behavior still apply.

## Handoff and Limitations

Selected endpoint: Spec → Slice only. Local CONTEXT/ADR/spec/ticket writes are authorized; no production changes, implementation/QA execution, GitHub issue mutation, staging or commit is included. New qualifying knowledge has been recorded in ADR 0069 and the Command Palette context entry; existing registry/ownership decisions were reused rather than duplicated. No blocking requirement decision remains for this bounded scope. Incidental API/layout choices remain with implementation; a new domain or scope decision must return to refinement rather than be silently invented by a ticket.

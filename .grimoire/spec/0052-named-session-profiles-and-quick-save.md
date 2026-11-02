# Named Session Profiles and Quick Save

**Spec ID:** 0052
**Status:** Accepted requirements; not implementation or runtime acceptance
**Date:** 2026-11-02
**Sources:** [Issue #157](https://github.com/wangjq4214/harbor/issues/157), inspected during refinement, and the owner's approvals of independent profiles, OSC 7 inheritance, fail-visible named launches, Spec → tickets, shared two-level palette selection, input-only naming with a Save button, internal confirmation, and the final capture/environment/write rules.
**Decisions:** [ADR 0106](../adr/0106-independent-session-profiles-and-explicit-osc7-launch-policy.md), [ADR 0107](../adr/0107-shared-palette-profile-flow-and-in-window-confirmation.md), [ADR 0108](../adr/0108-explicit-session-preset-snapshots-and-safe-configuration-writes.md).

## Outcome

A user selects a named shell launch preset from a new-session action or the command palette and gets one new session with the selected executable, argument array, initial directory and child-only environment overrides. The user can also turn the current session's known launch recipe and current directory into a new preset through naming, review and explicit confirmation, without opening a new native window. Editing or saving profiles never reconfigures existing child processes.

## Requirements

### R1 — Independent profiles and unchanged ordinary launches

Provide named profiles with stable identity, display name, executable, argument array, starting directory and environment overrides in the selected TOML configuration. Profiles do not inherit other profiles or `[shell]`. Ordinary startup and ordinary New tab/Ctrl+T retain the existing `[shell]` behavior, including its current executable fallback and directory rules; selecting a profile is an explicit different action.

Reuse centralized launch validation and platform argument quoting; do not construct shell command strings in UI callbacks. Preserve argument boundaries for empty strings, spaces, quotes, backslashes and Unicode. After choosing a named profile's executable, inability to start it is a visible failure, not a fallback to another shell. A failed new session must not disturb existing sessions.

Concrete TOML names, stable-ID representation, command-ID spelling and incidental Rust signatures remain implementation details, to be documented consistently. This contract does not select inheritance or a new fallback policy through those details.

### R2 — Literal environment, directory validation and coherent reload

Retain [spec 0024 R2–R4](./0024-watched-reload-and-launch-settings.md) and ADR 0062's guarantees for profile launch fields: literal child-only environment overrides; Windows case-insensitive names/conflict rejection; explicit empty values; removal restores inheritance rather than deleting inherited variables; protected `TERM=xterm-256color`; no value disclosure in diagnostics; configuration-relative profile paths; no process-global environment or cwd mutation.

Profile validity participates in the existing complete-candidate manual/watched reload boundary. An invalid profile cannot partially publish other profiles, colors, fonts, bindings or future launch policy. A successful acceptance publishes a coherent profile catalog and future launch state; removal is not a merge with an old generation. Existing children retain identity, environment and current directory. Invalid configured paths reject the complete reload; a selected configured directory disappearing before spawn causes visible failed creation, not fallback.

Keep startup recovery separate from strict live acceptance; do not claim a malformed profile successfully launched by silently substituting another recipe. Preserve existing startup behavior outside the new profile fields.

### R3 — Explicit Host-owned OSC 7 directory inheritance

For a named launch, resolve directory sources by the following precedence. This does not permit bypassing an invalid explicitly supplied or configured directory:

1. Valid explicit launch directory.
2. Configured profile directory.
3. Usable local OSC 7 directory from the source session.
4. Harbor's inherited process directory.

An absent or unusable OSC 7 candidate contributes no directory; it must not defeat a higher-priority explicit/configured directory. The Host must convert/validate the URI safely for Windows, including percent-encoded text and drive-path URI form. Do not treat a raw URI path as an already-native `PathBuf`, contact a remote host/share to test it, or let terminal output change any existing process directory. Reject remote/nonlocal inheritance without implicit network access. Honor existing metadata reset/invalid-input behavior from [ADR 0033](../adr/0033-osc7-metadata-without-filesystem-side-effects.md).

This adds explicit user-action-driven Host policy for named launches, not parser-driven filesystem effects or a change to ordinary `[shell]` launch behavior. Creation still reports a selected directory becoming unavailable at spawn rather than pretending success.

### R4 — Shared two-level palette selection

The profile new-session entry and command-palette command reach the same profile chooser. In the command palette, selecting the profile-opening action shows a second-level profile selection view in the same panel, with a meaningful title, a return control and the existing input field used to filter choices. Selecting an available profile launches it once and focuses the resulting terminal. Empty/no-match results cannot launch a phantom selection or send Enter to the old terminal.

Reuse the palette's existing editing and matching conventions, current availability, source-aware execution and Host command path from [spec 0030](./0030-command-palette.md). Recheck the target profile/current accepted state before invocation: removal or change during selection cannot execute an obsolete recipe or unrelated target. Accepted catalog updates reach both entry paths; a rejected reload leaves both unchanged.

This is navigation within one profile/palette flow, not two independently active panels or a general suspended-popup stack. Preserve [spec 0033](./0033-exclusive-extensible-application-popups.md): replacing a participating popup retires its input lifetime, and closing a replacement does not resurrect displaced search or stale editors. Existing native paste-confirmation windows remain unchanged and separate.

### R5 — Input-only naming, Save button and internal confirmation

Expose Save current session as preset through the command palette and new-session menu. Open a palette-style naming presentation with no command/result list. Place a `Save…` button to the right of the name input, on the same row inside the panel. Match the input's height, corner treatment and material while retaining visible button hover/press feedback. This is not a full graphical profile editor.

The button and Enter perform the same action: validate the draft name, then advance to confirmation; neither writes a file. Empty or duplicate names receive nearby feedback and cannot proceed through a disabled button or keyboard bypass. Existing presets are not overwritten on name collision.

Show confirmation as an internal overlay in the current application window, never a new winit/native window. Present the name, captured program/arguments and directory, and whether configured environment overrides will be included. Do not turn environment values into an unredacted review/log dump. Provide Return to edit and an explicit final Save action. Returning retains the name/draft; cancellation writes nothing. Failure retains the user's input for correction/retry.

Use the approved keyboard/focus safeguards: repeated Enter from advancing the naming step cannot also confirm saving; an IME candidate confirmation is not a flow submission; underlying input panel and terminal cannot receive confirmation-layer actions. Empty/invalid names cannot bypass checks via Enter. Retired editors, late clipboard/IME delivery and stale queued requests cannot modify or commit a replacement flow. After dismissal, restore eligible terminal focus without leaking consumed input to the PTY.

### R6 — Stable, known session capture rather than runtime scraping

At save initiation, capture the source session's Harbor-recorded launch program and arguments, plus its directory at that point: usable local OSC 7 first, otherwise that session's initial directory. Retain the information needed to identify the launch recipe and initial directory even after profile edits/reloads. The reviewed draft must not silently switch to another tab, later profile definition or later OSC 7 directory while the user names or confirms it.

Do not save typed command history, a guessed foreground/nested program recipe, or inferred runtime shell modifications. This is saving a reproducible known startup recipe, not cloning the current process state.

Environment overrides are excluded by default. The user may deliberately select existing configured override entries for inclusion and confirm that they contain no passwords/tokens. Do not capture all inherited system variables or inspect a running shell's environment. Credentials and automatic secret persistence remain outside scope; opting into configured overrides is not a general credential-storage feature. Keep full diagnostic chains and evidence free of environment values/secrets.

### R7 — Explicit, conflict-safe configuration persistence

Only final Save authorizes adding a new preset to the configuration file selected at application startup. Preserve unrelated settings and comments. Installed/unmarked mode uses its selected home configuration; portable mode uses only its selected executable-adjacent file, never falls back to home configuration or another writable path.

An externally changed document, invalid file contents, duplicate name discovered at commit, or write failure must result in visible feedback, no force overwrite, retained input and no loss of existing presets/unrelated edits. Protect the existing file from partial/truncated failed writes. Do not serialize an older accepted in-memory document over a newer file or silently remove comments. Revalidate conflict conditions at commit, not only when opening the flow.

A successful save is durably present in the selected configuration and available through the profile chooser under the same validation/publication contract as other accepted configuration. A queued request or temporary UI entry alone is not success. Save-triggered watcher/manual activity must not duplicate entries or publish stale generations; rejection cannot partially install other candidate fields. Do not claim the in-memory/disk operation completed if only one outcome occurred; report failure and retain the draft honestly.

These explicit writes do not authorize automatic startup/watch/reload writes, changes to configuration-path selection, or changes to portable/home isolation. See ADR 0108's qualification of ADR 0057.

### R8 — Scoped evidence and package tracking

Record focused tests plus reproducible Windows runtime evidence for delivered profile launching, palette navigation, IME/focus, the internal confirmation flow and save/reopen behavior. Record revision, dirty-tree scope, OS/ConPTY/application versions, steps, expected versus observed results, retrievable artifacts, exclusions and PASS / FAIL / NOT RUN / BLOCKED. Use synthetic values and directories; do not expose unrelated terminal contents or secrets.

Applicable gates from #157 are `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test --workspace`, `python scripts/check_docs.py`, and `python scripts/checklist_summary.py`. Record commands not run and why. Follow current [testing guidance](../../docs/developer/testing.md) and retain durable evidence under `docs/verification/` or retrievable CI artifacts as required by the issue; a requirements document is not execution evidence.

Update user/status/protocol documentation only for implemented and evidenced behavior, retaining limitations. #157 is a work-package tracker: link implementation sub-issues when execution starts, with separate authorization for remote mutation. Do not close it on this refinement or parser-only tests; evidence all accepted scope or explicitly link remaining/deferred work.

## Solution and Integration Seams

Reuse the existing configuration, application-command, exclusive-popup, editor, Host/factory and platform-PTY boundaries. Profile navigation, save draft meaning and persistence policy remain application-owned; reusable widgets must not acquire Harbor profile/session dependencies. No second command bus, arbitrary shell-string executor or new native confirmation window is selected.

| Seam | Connects | Expects | Provides |
| --- | --- | --- | --- |
| Accepted settings | `harbor-config` → application acceptance/factories/catalog | Complete validated profile collection and literal launch fields | Coherent future policy and discoverable choices; rejection retains old generation |
| Invocation | New-session entry/palette → typed action path → Host | Current profile identity, valid source context and explicit launch intent | One admitted execution, truthful failure, resulting terminal focus |
| Directory policy | Source-session OSC 7 metadata/profile/explicit directory → Host → PTY | Local validated native directory, preserving precedence | New-child-only cwd; no implicit remote access, global chdir or shell `cd` injection |
| Capture | Host-owned session provenance → save draft | Known launch program/arguments and initial directory plus save-time metadata | Stable source snapshot through tab/config/directory changes |
| UI lifecycle | Shared palette/editor/popup owner → naming/confirmation | One active interaction lifetime and owned focus/input | Two-level chooser, input-only naming, equivalent mouse/keyboard advance, no native window or PTY leakage |
| Persistence | Confirmed draft + selected on-disk file → config writer/acceptance/watch | Conflict-checked current document, name and explicit environment choices | Non-destructive write, preserved comments/settings, durable accepted profile or honest failure |

Existing surfaces include `crates/harbor-config/src/lib.rs`, `crates/harbor-app/src/{command,palette,popups,appearance,tab_manager}.rs`, `src/{tab_coordinator,shell}.rs` and `crates/harbor-pty/src/windows.rs`. These are inspected integration points, not mandatory new module/type layouts.

## End-to-End Acceptance

| ID | Given / when | Required observation |
| --- | --- | --- |
| E1 | Configure two independent profiles; launch each through new-session selection and palette second level | One intended child per invocation, exact argv/env/cwd, resulting terminal focus; no accidental `[shell]` inheritance |
| E2 | Continue ordinary startup/New tab/Ctrl+T with existing `[shell]`; attempt a named profile whose executable fails | Legacy behavior unchanged; named launch visibly fails without substituting another shell or harming old sessions |
| E3 | Use empty/quoted/spaced/Unicode/backslash arguments; environment literal/empty/removal/case conflicts and protected TERM | Real child receives exact accepted inputs; invalid complete candidates reject; diagnostics reveal no environment values |
| E4 | Exercise each directory precedence level, percent-encoded local drive paths, remote/malformed/reset/missing OSC 7, and a higher-priority directory | Correct cwd or deliberate unusable-metadata fallback; no remote access, global cwd change or existing-child mutation |
| E5 | Resolve a relative profile path from a config outside Harbor's launch cwd; remove an accepted directory before spawn, then restore | Config-relative resolution; visible failure without fallback; later valid creation works |
| E6 | Accept profile add/edit/remove, reject invalid candidate mixed with live fields, and invoke a selection retained across reload | Catalog/factories coherent; old children unchanged; obsolete/deleted selection cannot execute old recipe; rejected candidate leaks no changes |
| E7 | Filter and return within the second-level chooser; use empty/no-match results; replace search/palette and deliver stale input | One active panel, valid selection/back navigation, no phantom child/newline, no resurrected search or stale editor/action |
| E8 | Start save from both entry paths; enter name; use button and Enter separately; test empty/duplicate names | Same input-only naming layout and advance action, styled right-hand button, no write before final confirmation, invalid names cannot advance |
| E9 | Advance with held/repeated Enter, Chinese IME candidate confirmation, mouse and keyboard; cancel or return to edit | No unintended save, no new native window, review/return/cancel works, lower UI and PTY receive none of the consumed input |
| E10 | Start save, then change active tab/OSC 7/profile settings; include a source without OSC 7 | Review preserves captured source program/arguments/save-time cwd or source initial cwd, not later state/history/nested program |
| E11 | Save with default environment choice, then explicitly select synthetic configured overrides and confirm | Default file contains no captured environment; opt-in includes only selected overrides, no inherited/runtime environment or value leakage |
| E12 | Confirm a new name into a commented file containing unrelated settings; reopen chooser and restart Harbor | Preserved unrelated settings/comments, new preset persisted and selectable/reloadable, no duplicate publication; its launch reproduces the saved recipe |
| E13 | Change file externally or introduce a name collision after review; use invalid TOML, denied write and an interrupted/failed replacement | Existing content/presets preserved, no force overwrite or success claim, visible error and retained editable draft; valid retry can succeed |
| E14 | Perform save in installed and portable modes, with watcher/manual activity around the write | Only selected file targeted, portable home config untouched, coherent accepted state/no duplicate or stale entries, old sessions unchanged |

## Decisions and Traceability

- R1–R3/E1–E6: #157 plus approved independent-profile/legacy/fail-visible/OSC 7 direction (ADR 0106); inherited literal environment and acceptance rules are spec 0024/ADR 0062; OSC 7 parsing remains ADR 0033.
- R4–R5/E7–E9: owner-selected second-level palette, input-only save, adjacent styled button and internal confirmation (ADR 0107). The owner accepted the recommended back navigation, meaningful review, return/retry, duplicate/empty feedback, repeated-Enter/IME safeguards and lower-layer input gating. Spec 0030/0033 and ADR 0078 retain existing palette availability, editor and exclusive-popup rules; no disabled-reason text is newly required.
- R6–R7/E10–E14: final owner approval of stable known launch capture, save-time OSC 7/initial directory, environment excluded by default with explicit configured-entry selection, and non-destructive selected-file writes (ADR 0108). User-requested saving is distinct from the automatic writes prohibited by ADR 0057.
- R8: #157's scoped evidence, quality-gate and work-package closure requirements. Existing spec acceptance does not prove this new functionality or a fresh runtime run.

## Verification Guidance

Cover complete configuration rejection/publication, safe URI-to-native conversion without network effects, actual argv/env/cwd at the child boundary, source provenance retained across reload, and real action-queue execution counts. Exercise capture → name → confirmation → writer → accepted chooser, not only isolated serialization/filter helpers. Inject external-edit/name races, malformed files, write/replacement faults and stale editor/clipboard/IME requests; preserve the on-disk original and draft on failure.

Run focused tests with each slice. Windows runtime coverage remains necessary for ConPTY, actual arguments/directories, native IME/focus and confirmation-without-new-window behavior. No new performance/latency threshold, arbitrary URI host allowlist, exact keyboard shortcut, concrete configuration schema spelling or filesystem locking algorithm is prescribed; implementations must meet the observable contracts without inventing user-visible policy.

## Out of Scope

- Credentials, automatic secret persistence, runtime process/environment cloning and command-history capture.
- Profile inheritance, a full graphical profile editor, implicit replacement of existing named presets, or changes to ordinary `[shell]` launch/fallback behavior.
- New native save-confirmation windows, conversion of existing paste-confirmation windows, a generic popup stack or new command bus.
- Reconnectable process services, workspace restoration, pane implementation or broad remote-directory workflows.
- Code implementation, runtime acceptance, remote issue mutation, commits or Git-policy changes during this refinement.

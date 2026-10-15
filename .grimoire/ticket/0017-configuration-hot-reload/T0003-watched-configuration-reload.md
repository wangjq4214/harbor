# Reload Automatically Across Saves and File Replacement

**Ticket ID:** T0003
**Source:** [Spec 0024 R4/R5, E8/E9/E11](../../spec/0024-watched-reload-and-launch-settings.md), [ADR 0062](../../adr/0062-watched-reload-and-new-session-launch-settings.md).
**Status:** Done — scoped controlled, real Windows filesystem/transaction and native-window evidence passed; combined package acceptance remains T0004.

## Goal

With watching enabled by default, ordinary saves, editor atomic replacements and save bursts reach the same strict transaction as manual reload; invalid edits and watcher faults remain recoverable and visible.

## Affected Surfaces

Application file-observation lifecycle, event routing, coordinator transaction invocation, UI/log feedback, filesystem/event-order tests and user documentation. Do not extend widget-library HMR.

## Approach

Treat file observation as a source of reload requests, not a configuration owner. Coalesce saves and maintain coherent request/acceptance ordering through the established application thread. Keep manual invocation working independently. Choose and document watcher backend/timing without inventing a product latency guarantee.

## Dependencies and Coordination

- **Blocked by:** No local ticket; uses existing manual transaction baseline.
- **Blocks:** T0004 combined acceptance.
- T0002 is parallel with shared-file coordination. Verify initial watcher behavior using existing colors/bindings/fonts; T0004 covers environment/directory integration.

## Acceptance

- [x] Watching is active by default without widget-HMR or palette prerequisites.
- [x] Real Windows saves, replacement/rename saves, subsequent saves and bursts ending in valid content apply final stable settings, without detaching observation.
- [x] Intermediate missing/malformed files never publish defaults; a stable invalid file reports failure and retains the prior accepted state; later correction succeeds.
- [x] Repeated identical failures avoid notification flooding without hiding changed errors or recovery.
- [x] Manual/save interleaving does not let stale work overwrite a later accepted configuration; current applicable sessions and future factories agree.
- [x] Watcher setup/runtime failures are visible and manual reload remains usable.
- [x] No PTY restart, session recreation, second acceptance policy or value-leaking diagnostics are introduced.
- [x] E8/E9/E11 and interleaving with existing live fields have controlled and real-filesystem evidence; applicable gates and limitations are recorded.

## Execution Result

Implemented default-on content polling (200 ms sample / 300 ms settling, not a latency promise), one latest-observation mailbox and payload-free application wake, and shared strict current-file reload. Manual invocation remains independent. Automatic identical errors are suppressed without hiding changed errors, observation recovery or remaining invalid settings after a watcher fault. Observation never owns accepted settings; no dependency or widget-HMR change was needed.

The local-only report `docs/verification/watched-configuration-reload-evidence.md` (Git-ignored and not distributed) records baseline `2488f965b1efdc482ba0335b34adb137366b3f4f` plus the uncommitted watcher diff, Windows 26300.9457, native GPU/ConPTY/shell versions, reproduction, expected/observed outcomes and limitations. Five controlled/lifecycle tests and three real-filesystem tests passed. Native-window saves/replacements/bursts, rejection/suppression, runtime-fault feedback and correction were exercised; original child continuity and coherent current/future resource publication were checked by real transaction fixtures. Review found and fixed two feedback-order defects before final verification.

All five standard gates passed. Workspace total: 2464 passed, 0 failed, 10 ignored. Formatting and Clippy passed; a nonfatal incremental-cache finalization note did not fail the command. Documentation/link and protocol-inventory scripts passed. Temporary owned config/processes were removed. No remote mutation, staging, commit, ignore-policy edit or full-package closure occurred.

Combined environment/directory E10, the inherited broad desktop matrix and remote issue/PR reconciliation remain T0004 work. Synthetic desktop text entry was unreliable; the evidence explicitly distinguishes native footer/font observations from automated PTY/input-continuity checks. The local plan `.grimoire/plans/0030-watched-configuration-reload.md` remains ignored under existing policy; the validation report is also local-only under `docs/verification/`, as explicitly requested by the user after completion. The user waived the tracked/retrievable-report requirement for this T0003 execution; this does not waive T0004's package evidence requirements.

## Out of Scope

New launch-field implementation, changing startup fallback, widget HMR, command palette, arbitrary latency promises or a new configuration ownership architecture.

# Watched Configuration Reload and New-Session Launch Settings

**Spec ID:** 0024
**Status:** Draft
**Date:** 2026-10-15
**Sources:** [Issue #155](https://github.com/wangjq4214/harbor/issues/155), [manual slice #172](https://github.com/wangjq4214/harbor/issues/172), [spec 0023](./0023-atomic-manual-configuration-reload.md), [ADR 0062](../adr/0062-watched-reload-and-new-session-launch-settings.md), and the conversation contract below.

## Authority and Scope

- **C1:** The user explicitly included both environment variables and launch directory in the current #155 package; neither is deferred.
- **C2:** The user approved inherited-environment overlays, Windows case-insensitive name conflicts, empty-string values, removal restoring inheritance, literal values, protected `TERM`, configuration-relative directories, complete rejection of invalid reload candidates, honest later spawn failure, and variable-value confidentiality.
- **C3:** The user approved automatic watching enabled by default, stable-error feedback without repeated identical notification flooding, and the spec-to-ticket endpoint.
- **C4:** This work organizes the remaining delivery and existing manual-reload acceptance. It does not authorize implementation, test execution of the product, staging/committing, or remote issue changes during refinement.

This spec extends spec 0023 rather than replacing its manual command, font, OSC, input, startup compatibility, and failure contracts. Its environment/directory and watcher exclusions remain true for that initial slice but not for this extension. Shell program/arguments remain new-session-only; clipboard policy remains restart-required. Existing ADR 0059/0060/0061 qualifications continue to apply.

### Observed Baseline, Not Acceptance

At inspection, branch `feat/config-reload` at `bdb37993e0cfa95fa4708596ebacf8d2dee01e4c` contains manual reload and terminal font transitions in [PR #201](https://github.com/wangjq4214/harbor/pull/201). The PR is open; #172 still excludes fonts in its published wording. The local status page says interactive Windows reload acceptance is NOT RUN; no corresponding reload evidence record was available in this checkout at inspection. `/docs/verification/*` is ignored. These observations are not proof that checks failed or that evidence cannot exist elsewhere; delivery must locate retrievable evidence or produce it.

## Requirements

### R1 — Retain and Accept the Existing Manual Contract

Preserve all requirements and E1–E11 acceptance cases of spec 0023. Manual `app.reload-configuration` remains available through its configured binding, independent of a command palette or the watcher. Colors, bindings and terminal-only fonts update all applicable existing and future instances; invalid candidates retain the previous accepted state. Keep OSC overrides/reset-baseline semantics, bounded resize, reader resume and explicit compensation-failure reporting. Do not restart PTYs or recreate sessions.

Before claiming #172 delivered, reconcile its published font exclusion with the explicitly approved font-inclusive local contract. Documentation and issue status must distinguish implementation, executed tests, and runtime acceptance.

### R2 — Environment Overrides for New Sessions

Expose a TOML string-name/string-value environment mapping as part of shell launch settings. Concrete field names are not fixed by this contract; use one documented schema consistently across configuration, diagnostics, examples and tests.

- Start with the environment inherited by Harbor and apply configured additions/overrides only to the child launch environment. Do not change Harbor's process-global environment or existing child environments.
- On Windows, names are case-insensitive; conflicting configured names differing only in case reject the candidate instead of depending on map iteration order.
- An empty string is an explicit empty value. Removing an override from the complete settings document restores inheritance; it does not remove the inherited variable. No dedicated inherited-variable deletion syntax is required in this scope.
- Values are literal strings. Do not interpolate `$VAR` or `%VAR%`, or evaluate shell expressions.
- Preserve `TERM=xterm-256color`. A configured different value, regardless of key casing on Windows, is rejected rather than silently ignored. The same value is compatible with the protected policy.
- Invalid names/values or invalid mapping types reject the complete reload under R4. Preserve Windows environment-block validity at the native process boundary.
- User-facing feedback, logs and error chains may identify a variable name but must not disclose its value. Use synthetic non-secret values for test evidence.

Successful startup and successful reload must pass the configured environment to newly created sessions, including the actual shell selected by the existing fallback policy. This does not redefine executable lookup or shell fallback order.

### R3 — Initial Directory for New Sessions

Expose an optional launch-directory path in shell launch settings; its concrete TOML field name is left to implementation alongside R2's mapping.

- Absolute paths designate the initial child directory. Resolve relative paths against the configuration file's containing directory, not Harbor's launch location or the active terminal's current directory.
- Omission retains current inherited-directory behavior. Removing a previous setting from a valid complete document restores that behavior for future sessions.
- Do not change Harbor's process-global directory or send `cd` text into existing sessions. Do not reinterpret OSC 7 metadata as the configured directory; ADR 0033 remains unchanged.
- Invalid, nonexistent or non-directory configured paths reject the complete reload candidate. Errors identify the relevant setting and operation.
- Directory validity can change after acceptance. If it is unavailable when a new child is created, fail that session creation visibly, without silently launching in another directory or disturbing existing sessions. A failed new-session attempt is not a successful spawn and is not a retroactive configuration-generation change.

Valid startup settings apply to the first session as well as subsequent sessions. Retain the documented startup fallback behavior separately from strict reload; this extension does not turn startup recovery into live acceptance.

### R4 — One Acceptance Boundary, Including Future Launch Policy

Manual and automatic requests validate a complete candidate, including environment and directory settings, before publishing supported live state and future factories on the application thread. A failure in a new launch field must not accept changed colors, bindings, fonts, shell program or arguments. A resource/resize failure must not publish new launch settings either. The existing compensation-failure exception does not authorize partial configuration publication.

On success, feedback accurately distinguishes live changes, new-session-only program/arguments/environment/directory changes, and restart-required settings. Existing children retain their identities, launch environment and current working directories. New instances use the accepted launch policy together with the accepted appearance/font policy. Removal of optional overrides uses the complete-document semantics, not a merge over an older candidate.

### R5 — Default-On Watching and Save Recovery

Automatically observe the user configuration path without requiring a manual command. Retain the manual trigger and keep this feature separate from debug-only widget-library HMR.

- Ordinary saves, replacement/rename-based atomic saves, and bursts of saves must lead to the final stable valid document being applied through R4. Replacing the file must not detach future observation.
- Debounce save bursts; intermediate unreadable or malformed files must never substitute startup defaults into live state. If an invalid edit persists, retain the last accepted configuration and show actionable feedback. Later correction must be observed and can succeed without application restart.
- Repeated identical failures must not flood notifications. This suppression must not hide recovery or a different failure.
- Manual requests interleaved with save events must preserve coherent acceptance order. A stale candidate must not overwrite a later accepted configuration due to delayed work; all applicable instances and factories agree after each acceptance.
- Watcher setup/runtime failure is visible, not silent. Manual reload remains usable when automatic observation is unavailable.

No numeric debounce threshold or specific watcher library is prescribed. Choose and document the implementation behavior and verify it using controlled save sequences; do not claim a latency budget not established here.

### R6 — Package Evidence and Scope Reconciliation

Close #155 only after R1–R5 are evidenced, or through a subsequent explicit scope decision linking every deferred item. Environment and launch directory are included now, not deferred by this spec. Do not add a dependency on reflow for watcher/environment work: the font slice already uses the existing resize contract, and #152 is closed at inspection.

Apply [validation policy](../../docs/validation.md) and the quality gates from #155. Record revision, dirty scope, OS/ConPTY/application versions, steps, expected/observed outcomes, artifacts and exclusions with PASS / FAIL / NOT RUN / BLOCKED. Provide retrievable evidence rather than a local-only missing/ignored Markdown link. If ignore policy prevents committing an intended evidence file, use allowed retrievable artifacts or obtain separate authorization; do not silently edit Git policy.

## Solution and Seams

Extend the existing application-owned configuration transaction and child-launch boundary. File observation supplies reload requests; it is not a second configuration state owner. The existing manual acceptance contract remains authoritative for resource preparation and publication.

| Seam | Connects | Expects | Provides |
| --- | --- | --- | --- |
| Candidate parsing | Config file / `harbor-config` → application | Complete typed launch fields and diagnostics, plus config-file location | Strict validation inputs and stable relative-directory meaning without exposing variable values |
| Reload triggers | Manual dispatcher and watcher → application coordinator | Current-file reload requests, save coalescing and failure notification | One ordered acceptance path, not independent state writers |
| Accepted launch policy | Application coordinator → tab factory | Fully validated candidate and successful existing resource transaction | Future launches consistent with accepted live settings; no publication on failure |
| Child creation | Factory / `ShellCommand` → Windows PTY backend | Literal environment overlays and accepted initial directory | Valid per-child environment/directory, existing process lifecycle, explicit spawn errors |
| Feedback | Reload/watch/spawn outcomes → UI and logs | Actual result and application policy | Non-modal actionable diagnostics, no environment values, no PTY injection |

Current integration surfaces: [config](../../crates/harbor-config/src/lib.rs), [coordinator](../../src/shell.rs), [tab factory](../../src/tab_coordinator.rs), [PTY launch inputs](../../crates/harbor-pty/src/lib.rs), [Windows process creation](../../crates/harbor-pty/src/windows.rs), and [manual transaction tests](../../src/configuration_reload_tests.rs). These references identify responsibilities, not a mandated internal refactoring plan.

## End-to-End Acceptance

| ID | Given / When | Required outcome |
| --- | --- | --- |
| E1 | Existing sessions; manually reload valid environment and directory settings; create another session | Existing PIDs, environments and current directories are unchanged; the new child reports the accepted values/directory and matching accepted colors/fonts |
| E2 | Inherited synthetic variable; configure override, empty string, then remove the override in successive reloads | New children observe override, explicit empty value, then inherited value; old children retain their own state |
| E3 | Windows case variants, conflicting names, invalid mapping values, and conflicting TERM; include a valid color change | Invalid candidates reject wholly; no live/factory changes; diagnostics identify the field without revealing values; matching protected TERM is valid |
| E4 | Literal values containing `$VAR`, `%VAR%` and shell-expression text; launch a child probe | Child receives exact strings, with no configuration interpolation/evaluation |
| E5 | Start Harbor from a different directory; configure a relative directory, then an absolute directory, then omit it | New sessions use config-relative, absolute, then inherited-directory behavior; OSC 7 and active-session directory do not redefine the baseline |
| E6 | A nonexistent path or a regular file is configured alongside valid live edits | Reload fails entirely with actionable diagnostics; existing sessions and future launch policy remain on accepted settings |
| E7 | Accept a valid directory; remove it before opening a tab; later restore it | Failed spawn is reported without fallback directory or damage to existing sessions; a later valid spawn uses the accepted policy |
| E8 | Default running app; ordinary save, rename-based replacement, another save and a burst ending in a valid document | No manual command needed; final stable settings reach current applicable views and future sessions; replacement does not break later observation |
| E9 | A save burst exposes a missing/invalid intermediate file; leave an invalid file stable, repeat the failure, then fix it | Last valid state survives; persistent error is visible without identical-message flooding; corrected content is accepted |
| E10 | Interleave manual reloads and automatic saves with colors/fonts/environment/directory changes and injected preparation failures | Accepted generations remain coherent; stale work cannot replace later accepted state; failure preserves prior launch/live settings and reader-resume guarantees |
| E11 | Controlled watcher setup/runtime failure; then invoke manual reload with a valid document | Watch failure is visible; manual reload still works without pretending automatic watching succeeded |
| E12 | Valid startup launch fields, plus existing startup fallback regression fixtures | First child receives configured launch settings; existing documented startup fallback remains separate from strict live validation |

## Delivery Verification

- Reuse spec 0023 E1–E11 and its actual Windows shell, inactive-tab, alternate-screen, input/IME, OSC, font and failure-compensation requirements; do not substitute new watcher tests for them.
- Test schema validation, case handling, literal/empty/removal semantics, relative path anchoring, environment-block construction, candidate/factory rollback, and redaction through complete error chains.
- Use actual Windows child probes for environment/directory and real filesystem saves/replacements for watcher behavior. Controlled event tests supplement, not replace, those paths.
- Run `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test --workspace`, `python scripts/check_docs.py`, and `python scripts/checklist_summary.py` at delivery; report skipped/blocked gates honestly.
- Reconcile README/config examples, current-status/roadmap descriptions, and #155/#172/PR scope based on evidenced behavior. Remote edits require execution authorization; this spec does not make them.

## Decisions and Limits

R2–R5 derive from C1–C3 and ADR 0062; R1 and atomicity qualifications derive from spec 0023 and ADR 0059–0061; R6 derives from issue #155 and validation policy. There are no additional confirmed product assumptions. Concrete field names, watcher backend and debounce timing remain implementation details, not new behavioral requirements.

No existing-process restart or environment/directory mutation, shell-expression expansion, inherited-variable deletion syntax, widget HMR changes, palette UI, new profile system, live clipboard-policy changes, or changed OSC 7 side effects are introduced. This spec neither promises all historical content survives ordinary bounded font resize nor treats unavailable requested font family as necessarily fatal when the established usable-system-font fallback applies.

This document is a contract, not proof of implementation, successful tests, issue closure or Windows acceptance.

# Watched Configuration Reload and New-Session Launch Settings

**Spec ID:** 0024
**Status:** Accepted (owner-confirmed)
**Sources:** [#155](https://github.com/wangjq4214/harbor/issues/155), [#172](https://github.com/wangjq4214/harbor/issues/172), [manual spec 0023](./0023-atomic-manual-configuration-reload.md), [ADR 0062](../adr/0062-watched-reload-and-new-session-launch-settings.md). [Spec 0025](./0025-portal-and-configuration-notifications.md) qualifies only notification eligibility.

## Requirements
### R1 — Retain and Accept the Existing Manual Contract
Preserve spec 0023 R1–R8/E1–E11 and ADRs 0059–0061: configured manual command independent of watcher/palette, live colors/bindings/terminal-only fonts for applicable existing/future instances, strict rejection, OSC/reset/alpha, bounded resize, reader resume and explicit compensation-failure reporting. No session/PTY restart. Reconcile #172's published font exclusion before claiming the font-inclusive scope delivered; distinguish code, tests and runtime acceptance.

### R2 — Environment Overrides for New Sessions
Provide one documented TOML string-name/string-value shell environment mapping, consistently used in diagnostics/examples/tests:
- Start from Harbor's inherited environment; add/override only child launch values, including the actually selected fallback shell. Do not mutate Harbor or existing children, or change executable lookup/fallback order.
- Windows names are case-insensitive; conflicting case variants reject wholly. Empty string is explicit empty, removal restores inheritance rather than deleting inherited variables. No deletion syntax.
- Values are literal: no $VAR/%VAR% interpolation or shell evaluation.
- Protect TERM=xterm-256color case-insensitively: different value rejects, same value is valid.
- Invalid mapping types/names/values reject wholly; ensure a valid native Windows environment block.
- UI/log/error chains may identify names, never disclose values. Use synthetic evidence values.

### R3 — Initial Directory for New Sessions
Provide an optional shell launch-directory path. Absolute is literal; relative resolves against the configuration file's containing directory, not launch cwd, active-session cwd or OSC 7. Omission/removal restores inherited-directory behavior. Never change process-global cwd or inject cd into existing children; [ADR 0033](../adr/0033-osc7-metadata-without-filesystem-side-effects.md) remains unchanged.
Invalid/nonexistent/non-directory paths reject complete reload with setting/operation diagnostics. If an accepted directory disappears before spawn, fail the new session visibly without fallback or disturbing existing sessions; this is neither successful spawn nor retroactive generation change. Restored directory permits later valid spawn. Valid startup fields affect the first session too; preserve separate startup recovery.

### R4 — One Acceptance Boundary, Including Future Launch Policy
Manual/watch requests validate the complete document and prepare resources before application-thread publication of coherent live state and future factories. Invalid launch fields cannot partially accept colors/fonts/bindings/program/args; live resource/resize failures cannot publish launch policy, including compensation failure. Existing children keep identities/env/cwd; new children use matching accepted launch/font/color policy. Complete-document override removal is not a merge. When feedback is needed, distinguish live, new-session-only and restart-required effects; clipboard remains restart-required.

### R5 — Default-On Watching and Save Recovery
Observe the selected configuration path automatically, separate from widget HMR; manual reload stays independent.
Ordinary saves, rename/atomic replacement and bursts eventually apply the final stable valid document through R4; replacement cannot detach observation. Debounce intermediates without installing defaults. Persistent invalid/unreadable edits retain accepted state and actionable notice; later correction succeeds without restart. Missing-file silence follows 0025. Repeated identical failures cannot flood or immediately resurrect dismissed notices, but different failures/recovery remain distinguishable. Interleaved manual/save work must preserve acceptance order; delayed stale candidates cannot overwrite newer accepted generations. Setup/runtime observation failure is visible, manual reload remains usable.
No required numeric latency/library: document/test controlled sequences. Current implementation polls content at 200ms with 300ms settling, coalesces latest observations into a payload-free wake, and re-reads current file on the application thread; these are implementation details, not latency guarantees.

### R6 — Package Evidence and Scope Reconciliation
Environment/directory remain included in #155. No new reflow prerequisite (#152 was closed at inspection; font slice uses existing resize). Close #155 only with accepted whole-scope evidence or an explicit new scope decision linking every remaining item. Reconcile README/example/status/roadmap and #155/#172/[PR #201](https://github.com/wangjq4214/harbor/pull/201) scope based on evidence; remote mutation requires separate authorization.
Use validation policy: exact revision/dirty scope, OS/ConPTY/app versions, steps, expected/observed outcomes, artifacts/exclusions and PASS/FAIL/NOT RUN/BLOCKED. Self-contained summaries/retrievable artifacts are the general rule; the execution-specific waiver below is not a global policy change or permission to change ignore rules.

## End-to-End Acceptance
| ID | Scenario → required observation |
| --- | --- |
| E1 | Manual env/directory reload then new child → old PIDs/env/cwd unchanged; new child reports accepted values/directory and matching fonts/colors. |
| E2 | Override inherited synthetic value, set empty, remove override → new children observe override/empty/inherited, old children unchanged. |
| E3 | Case variants/conflicts, invalid mapping and conflicting TERM mixed with valid colors → wholly reject with value-free field diagnostics; matching protected TERM valid. |
| E4 | $VAR, %VAR% and expression-like values → actual child receives exact literal strings. |
| E5 | Launch Harbor from other cwd; relative, absolute, omitted directory → config-relative/absolute/inherited child behavior, no OSC 7/session-cwd baseline change. |
| E6 | Nonexistent path/regular file plus valid live fields → wholly reject; live/factory state retained. |
| E7 | Remove accepted directory before spawn, then restore → visible failed spawn without fallback/session damage, later spawn succeeds. |
| E8 | Default watcher; ordinary/replacement/another save/burst → final stable generation reaches applicable current/future instances without manual command or lost observation. |
| E9 | Missing/invalid intermediates, stable invalid file, repeated failure, correction → last valid state, actionable nonflooding errors, recovery. |
| E10 | Interleave manual/ordinary/atomic/burst saves changing all live/launch fields across active/inactive/alternate/new sessions; inject font/resource, invalid TOML/binding/launch and native resize/compensation failures → coherent ordering, rejection and reader/recovery guarantees; no PTY termination or partial publication. |
| E11 | Controlled setup/runtime watch fault then valid manual reload → visible limitation, manual still works, no false watch-success claim. |
| E12 | Valid startup launch fields plus startup-fallback regressions → first child uses accepted launch values, strict live policy remains separate. |

## Delivery and Outstanding Acceptance
Stable aliases `0017-configuration-hot-reload` (used by native test markers):
| Slice | Contract |
| --- | --- |
| T0001 — manual/font | 0023 R1–R8/E1–E11; this R1/R6 |
| T0002 — environment/directory | R2–R4/E1–E7/E12 |
| T0003 — watching | R4/R5/E8/E9/E11, manual/save ordering |
| T0004 — integrated package | All R/E, especially E10/R6 and inherited 0023 outcomes |

**Execution-specific waiver:** detailed evidence was required to remain untracked/local-only, replacing retrievable-evidence acceptance for that execution only, without ignore changes/force-add. The earlier slice waiver did not itself waive T0004. The local report was later deleted and is not available as remote evidence.

Regression scenarios include Chinese IME/preedit/candidate placement/focus cancellation, physical typing/Backspace/Enter and held-key rebind, retained selection/copy after repeated fonts, nvim alternate exit/primary restoration, backdrop/opaque/DPI and desktop OSC reset observations. Retain TERM/literal/empty/removal, relative cwd/lost-directory and full-chain confidentiality checks using actual Windows children and real replacement/save sequences.

Historical #172 font-exclusion wording differs from 0023/ADRs 0059–0061; #155/#172/PR #201 were not remotely reconciled by this cleanup.

## Limits
No existing-child restart/env/cwd mutation, interpolation, inherited-variable deletion, profile/palette UI, widget HMR change, live clipboard policy, OSC 7 side-effect change or special lossless font retention. Schema names/backend/timing remain implementation choices.

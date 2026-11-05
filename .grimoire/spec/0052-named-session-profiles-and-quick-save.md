# Named Session Profiles and Quick Save

**Spec ID:** 0052
**Status:** Accepted requirements; not implementation or runtime acceptance
**Date:** 2026-11-04
**Sources:** [Issue #157](https://github.com/wangjq4214/harbor/issues/157), inspected during earlier refinement; the owner's independent-profile, local OSC 7, palette, confirmation and safe-write approvals; and the settled 2026-11-04 refine handoff selecting environment-aware project presets, last recognized agents with original arguments, and a spec-only revision. See [CONTEXT: Session Profile Save](../CONTEXT.md#session-profile-save).
**Decisions:** [ADR 0106](../adr/0106-independent-session-profiles-and-explicit-osc7-launch-policy.md), [ADR 0107](../adr/0107-shared-palette-profile-flow-and-in-window-confirmation.md), [ADR 0108](../adr/0108-explicit-session-preset-snapshots-and-safe-configuration-writes.md) (capture partially superseded), [ADR 0109](../adr/0109-environment-aware-project-presets-and-recognized-agents.md) (partially superseded), and [ADR 0110](../adr/0110-last-recognized-agent-launch-with-original-arguments.md) (current correction). These records retain their recorded statuses; this spec expresses the settled owner handoff, not implementation or runtime acceptance.

## Outcome

A user selects an independent named launch preset from a new-session action or the command palette and gets one new session with its confirmed launch inputs. Quick Save creates a reusable project preset: identify the execution environment, capture the current project directory when Save starts, and remember/relaunch the last recognized supported agent with its original arguments, even after that agent exits. First-version coverage is local pwsh, bash inside WSL and bash via direct SSH. Naming, review and explicit confirmation stay within the current application window. Ordinary `[shell]` launches remain compatible; editing or saving profiles never reconfigures existing child processes.

## Requirements

### R1 — Independent profiles and unchanged ordinary launches

Provide named profiles with stable identity, display name, executable, argument array, starting directory and environment overrides in the selected TOML configuration. Profiles do not inherit other profiles or `[shell]`. Ordinary startup and ordinary New tab/Ctrl+T retain the existing `[shell]` behavior, including its current executable fallback and directory rules; selecting a profile is an explicit different action.

Reuse centralized launch validation and platform argument quoting; do not construct shell command strings in UI callbacks. Preserve argument boundaries for empty strings, spaces, quotes, backslashes and Unicode. After choosing a named profile's executable, inability to start it is a visible failure, not a fallback to another shell. A failed new session must not disturb existing sessions.

Concrete TOML names, stable-ID representation, command-ID spelling and incidental Rust signatures remain implementation details, to be documented consistently. This contract does not select inheritance or a new fallback policy through those details.

### R2 — Literal environment, directory validation and coherent reload

Retain [spec 0024 R2–R4](./0024-watched-reload-and-launch-settings.md) and ADR 0062's guarantees for profile launch fields: literal child-only environment overrides; Windows case-insensitive names/conflict rejection; explicit empty values; removal restores inheritance rather than deleting inherited variables; protected `TERM=xterm-256color`; no value disclosure in diagnostics; configuration-relative local profile paths; no process-global environment or cwd mutation.

Profile validity participates in the existing complete-candidate manual/watched reload boundary. An invalid profile cannot partially publish other profiles, colors, fonts, bindings or future launch policy. A successful acceptance publishes a coherent profile catalog and future launch state; removal is not a merge with an old generation. Existing children retain identity, environment and current directory. Invalid configured local paths reject the complete reload; a selected configured directory disappearing before spawn causes visible failed creation, not fallback.

Environment-aware preset fields participate in the same complete-candidate acceptance and coherent publication contract. Interpret the saved project directory in its identified environment, not through native-local path validation of a WSL/SSH directory. This does not authorize implicit network probes during validation or reinterpret remote OSC 7 as local. Concrete representation and validation mechanisms remain implementation choices within these guarantees.

Keep startup recovery separate from strict live acceptance; do not claim a malformed profile successfully launched by silently substituting another recipe. Preserve existing startup behavior outside the new profile fields.

### R3 — Explicit Host-owned OSC 7 directory inheritance

For a local named launch, retain the following directory-source precedence from ADR 0106. This is separate from R6's save-time project-directory capture and does not permit bypassing an invalid explicitly supplied or configured directory:

1. Valid explicit launch directory.
2. Configured profile directory.
3. Usable local OSC 7 directory from the source session.
4. Harbor's inherited process directory.

An absent or unusable OSC 7 candidate contributes no directory; it must not defeat a higher-priority explicit/configured directory. The Host must convert/validate the URI safely for Windows, including percent-encoded text and drive-path URI form. Do not treat a raw URI path as an already-native `PathBuf`, contact a remote host/share to test it, or let terminal output change any existing process directory. Reject remote/nonlocal inheritance without implicit network access. Honor existing metadata reset/invalid-input behavior from [ADR 0033](../adr/0033-osc7-metadata-without-filesystem-side-effects.md).

This adds explicit user-action-driven Host policy for local named launches, not parser-driven filesystem effects or a change to ordinary `[shell]` launch behavior. Creation still reports a selected directory becoming unavailable at spawn rather than pretending success. R6/R9's explicit WSL/SSH preset launches do not make arbitrary remote OSC 7 a native local directory or authorize implicit network probes; R3's local launch fallback is never evidence of the current directory for Save.

### R4 — Shared two-level palette selection

The profile new-session entry and command-palette command reach the same profile chooser. In the command palette, selecting the profile-opening action shows a second-level profile selection view in the same panel, with a meaningful title, a return control and the existing input field used to filter choices. Selecting an available profile launches it once and focuses the resulting terminal. Empty/no-match results cannot launch a phantom selection or send Enter to the old terminal.

Reuse the palette's existing editing and matching conventions, current availability, source-aware execution and Host command path from [spec 0030](./0030-command-palette.md). Recheck the target profile/current accepted state before invocation: removal or change during selection cannot execute an obsolete recipe or unrelated target. Accepted catalog updates reach both entry paths; a rejected reload leaves both unchanged.

This is navigation within one profile/palette flow, not two independently active panels or a general suspended-popup stack. Preserve [spec 0033](./0033-exclusive-extensible-application-popups.md): replacing a participating popup retires its input lifetime, and closing a replacement does not resurrect displaced search or stale editors. Existing native paste-confirmation windows remain unchanged and separate.

### R5 — Input-only naming, Save button and internal confirmation

Expose Save current session as preset through the command palette and new-session menu. Open a palette-style naming presentation with no command/result list. Place a `Save…` button to the right of the name input, on the same row inside the panel. Match the input's height, corner treatment and material while retaining visible button hover/press feedback. This is not a full graphical profile editor.

The button and Enter perform the same action: validate the draft name, then advance to confirmation; neither writes a file. Empty or duplicate names receive nearby feedback and cannot proceed through a disabled button or keyboard bypass. Existing presets are not overwritten on name collision.

Show confirmation as an internal overlay in the current application window, never a new winit/native window. Present the name, execution environment, save-time project directory, launch program/arguments, any recognized agent invocation to be saved and its arguments, and whether configured environment overrides will be included. Missing/unusable environment or directory recognition must be visible and correctable, not presented as confirmed data. Suspected credential arguments require the visible handling in R9; do not expose credentials or turn environment values into an unredacted review/log dump. Provide Return to edit and an explicit final Save action. Returning retains the name/draft and permits required corrections; cancellation writes nothing. Failure retains the user's input for correction/retry.

Use the approved keyboard/focus safeguards: repeated Enter from advancing the naming step cannot also confirm saving; an IME candidate confirmation is not a flow submission; underlying input panel and terminal cannot receive confirmation-layer actions. Empty/invalid names cannot bypass checks via Enter. Retired editors, late clipboard/IME delivery and stale queued requests cannot modify or commit a replacement flow. After dismissal, restore eligible terminal focus without leaking consumed input to the PTY.

### R6 — Stable environment-aware project capture

At save initiation, capture the source terminal's identified execution environment and its current project directory in that environment. First-version support is local pwsh, bash inside WSL and bash via a direct SSH connection; multi-layer nesting is deferred. A WSL/SSH directory is not a native local Windows path. Never silently substitute the original process/initial launch directory, the earlier agent invocation's directory, or stale/unknown metadata as a confirmed current directory. If environment or directory recognition is missing or unusable, show the limitation and allow user correction.

Guided shell integration is accepted to supply the required recognition; zero-configuration guessing is not required. WSL/SSH-side cooperation may be necessary. The concrete integration/capture mechanism, schema and installation procedure are implementation choices, not selected designs; this approval does not authorize unsolicited local or remote configuration changes or remote installation. Terminal metadata alone does not authorize network probes.

Retain source launch provenance and the relevant recognized information across configuration edits/reloads. Capture the last recognized supported agent under R9, not arbitrary foreground processes or the last ordinary command. The reviewed draft, including explicit corrections and confirmed arguments, must not silently change with later tab, directory, agent-record or configuration changes. No process-global cwd mutation, general history replay, runtime environment dump or process cloning is authorized. Virtual-environment recognition and activation, including venv/Conda/Poetry, are deferred and distinct from the configured override opt-in below.

Environment overrides are excluded by default. The user may deliberately select existing configured override entries for inclusion and confirm that they contain no passwords/tokens. Do not capture all inherited system variables or inspect a running shell's environment. Credentials and automatic secret persistence remain outside scope; opting into configured overrides is not a general credential-storage feature. Keep full diagnostic chains and evidence free of environment values/secrets.

### R7 — Explicit, conflict-safe configuration persistence

Only final Save authorizes adding a new preset to the configuration file selected at application startup. Preserve unrelated settings and comments. Installed/unmarked mode uses its selected home configuration; portable mode uses only its selected executable-adjacent file, never falls back to home configuration or another writable path.

An externally changed document, invalid file contents, duplicate name discovered at commit, or write failure must result in visible feedback, no force overwrite, retained input and no loss of existing presets/unrelated edits. Protect the existing file from partial/truncated failed writes. Do not serialize an older accepted in-memory document over a newer file or silently remove comments. Revalidate conflict conditions at commit, not only when opening the flow.

A successful save is durably present in the selected configuration and available through the profile chooser under the same validation/publication contract as other accepted configuration. A queued request or temporary UI entry alone is not success. Save-triggered watcher/manual activity must not duplicate entries or publish stale generations; rejection cannot partially install other candidate fields. Do not claim the in-memory/disk operation completed if only one outcome occurred; report failure and retain the draft honestly.

These explicit writes do not authorize automatic startup/watch/reload writes, changes to configuration-path selection, or changes to portable/home isolation. See ADR 0108's qualification of ADR 0057.

### R8 — Scoped evidence and package tracking

Record focused tests plus reproducible Windows runtime evidence for delivered profile launching, palette navigation, IME/focus, the internal confirmation flow and save/reopen behavior. Expanded support additionally requires runtime evidence for local pwsh, WSL bash and direct-SSH bash, save-time directory correctness/unknown handling, and last-recognized pi, Codex, Claude Code and OpenCode launches with original argument meaning. Record revision, dirty-tree scope, OS/ConPTY/application and relevant shell/environment/agent versions, steps, expected versus observed results, retrievable artifacts, exclusions and PASS / FAIL / NOT RUN / BLOCKED. Use synthetic values and directories; do not expose unrelated terminal contents or secrets. This revision specifies required evidence only; it neither runs new tests/QA nor claims expanded support exists.

Applicable gates from #157 are `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test --workspace`, `python scripts/check_docs.py`, and `python scripts/checklist_summary.py`. Record commands not run and why. Follow current [testing guidance](../../docs/developer/testing.md) and retain durable evidence under `docs/verification/` or retrievable CI artifacts as required by the issue; a requirements document is not execution evidence.

Update user/status/protocol documentation only for implemented and evidenced behavior, retaining limitations. #157 is a work-package tracker: link implementation sub-issues when execution starts, with separate authorization for remote mutation. Do not close it on this refinement or parser-only tests; evidence all accepted scope or explicitly link remaining/deferred work.

### R9 — Last recognized agent and original-argument relaunch

Remember the last recognized invocation of pi, Codex, Claude Code or OpenCode, with its original argument boundaries and meaning. Scope the record to the current terminal and execution environment, not globally across tabs or SSH hosts. The agent may already have exited when Save starts. Ordinary commands do not replace the record; a later recognized supported-agent invocation replaces the prior one in that scope. There is no requirement to recover persisted command history from before shell integration starts, or to recognize arbitrary aliases/wrappers.

On reopening the saved preset, enter its saved execution environment and project directory, then launch the selected supported agent with the confirmed arguments. Preserve empty arguments, spaces, quotes, backslashes and Unicode across environment boundaries as well as local launch. Honor original resume options: do not force a fresh session and do not add resume options. This is not general command-history replay, arbitrary last-command execution or a manually defined startup-script feature.

Credentials must not be directly persisted. Detected suspected password/token arguments require visible user handling before persistence, not silent removal followed by a claim that the exact invocation was saved. Any user-confirmed changes must remain visible as changes to the invocation; do not claim perfect secret detection. The existing configured-override opt-in and confidentiality rules remain independent of argument handling and of deferred virtual-environment capture.

## Solution and Integration Seams

Reuse the existing configuration, application-command, exclusive-popup, editor, Host/factory and platform-PTY boundaries. Profile navigation, save draft meaning and persistence policy remain application-owned; reusable widgets must not acquire Harbor profile/session dependencies. No second command bus, arbitrary shell-string executor or new native confirmation window is selected.

| Seam | Connects | Expects | Provides |
| --- | --- | --- | --- |
| Accepted settings | `harbor-config` → application acceptance/factories/catalog | Complete validated profile collection and literal launch fields | Coherent future policy and discoverable choices; rejection retains old generation |
| Invocation | New-session entry/palette → typed action path → Host | Current profile identity, valid source context and explicit launch intent | One admitted execution, truthful failure, resulting terminal focus |
| Local directory policy | Source-session OSC 7 metadata/profile/explicit directory → Host → PTY | Local validated native directory, preserving R3 precedence | New-child-only cwd; no implicit remote access, global chdir or shell `cd` injection |
| Capture and review | Source-terminal provenance + guided integration → application-owned save draft | Identified execution environment, current directory at Save initiation, scoped recognized-agent record and original arguments, with recognition limitations explicit | Stable review/correction through tab/config/directory changes; no initial-cwd fallback presented as current |
| Recognized-agent lifetime | Supported invocation recognition → terminal/environment-scoped record → save draft | pi, Codex, Claude Code or OpenCode launch information; original argument boundaries | Last supported invocation survives exit and ordinary commands; later supported invocation replaces it without cross-tab/host mixing |
| Environment-aware relaunch | Accepted confirmed preset → Host launch boundary → selected environment/project/agent | Explicit saved environment and directory, selected supported agent and confirmed arguments | Reopen in that environment and directory before agent launch; no native-local reinterpretation, credential persistence or arbitrary script executor |
| UI lifecycle | Shared palette/editor/popup owner → naming/confirmation | One active interaction lifetime and owned focus/input | Two-level chooser, input-only naming, equivalent mouse/keyboard advance, no native window or PTY leakage |
| Persistence | Confirmed draft + selected on-disk file → config writer/acceptance/watch | Conflict-checked current document, name and explicit environment choices | Non-destructive write, preserved comments/settings, durable accepted profile or honest failure |

Existing surfaces include `crates/harbor-config/src/lib.rs` for profile serialization; `src/profile_save.rs` for capture; `src/profile_launch.rs` for native local launch validation; `src/shell.rs` for save/write/publication; `crates/harbor-app/src/quick_save.rs` for UI; the existing command/palette/popup/tab-manager surfaces; and the platform PTY boundary. The inspected current capture uses latest OSC 7 via `active_working_directory` and `profile_launch::local_directory`, otherwise the initial launch directory. That is a current implementation fact, not the required R6 behavior or evidence of correctness. These seams do not select a new module layout, shell-integration protocol, recognition algorithm or persistence schema.

## End-to-End Acceptance

| ID | Given / when | Required observation |
| --- | --- | --- |
| E1 | Configure two independent profiles; launch each through new-session selection and palette second level | One intended child per invocation, exact argv/env/cwd, resulting terminal focus; no accidental `[shell]` inheritance |
| E2 | Continue ordinary startup/New tab/Ctrl+T with existing `[shell]`; attempt a named profile whose executable fails | Legacy behavior unchanged; named launch visibly fails without substituting another shell or harming old sessions |
| E3 | Use empty/quoted/spaced/Unicode/backslash arguments; environment literal/empty/removal/case conflicts and protected TERM | Real child receives exact accepted inputs; invalid complete candidates reject; diagnostics reveal no environment values |
| E4 | Exercise each local named-launch directory precedence level, percent-encoded local drive paths, remote/malformed/reset/missing OSC 7, and a higher-priority directory | Correct local launch cwd or deliberate R3 unusable-metadata fallback; no implicit remote access, global cwd change or existing-child mutation; fallback is not a Save-time current-directory claim |
| E5 | Resolve a relative local profile path from a config outside Harbor's launch cwd; remove an accepted directory before spawn, then restore | Config-relative resolution; visible failure without fallback; later valid creation works |
| E6 | Accept profile add/edit/remove, reject invalid candidate mixed with live fields, and invoke a selection retained across reload | Catalog/factories coherent; old children unchanged; obsolete/deleted selection cannot execute old recipe; rejected candidate leaks no changes |
| E7 | Filter and return within the second-level chooser; use empty/no-match results; replace search/palette and deliver stale input | One active panel, valid selection/back navigation, no phantom child/newline, no resurrected search or stale editor/action |
| E8 | Start save from both entry paths; enter name; use button and Enter separately; test empty/duplicate names | Same input-only naming layout and advance action, styled right-hand button, no write before final confirmation, invalid names cannot advance |
| E9 | Advance with held/repeated Enter, Chinese IME candidate confirmation, mouse and keyboard; cancel or return to edit | No unintended save, no new native window, review/return/cancel works, lower UI and PTY receive none of the consumed input |
| E10 | Start Save after changing project directory; then change active tab/directory/agent record/profile settings; separately use missing, stale or unusable environment/directory recognition | Review preserves the source environment, Save-time cwd and scoped agent snapshot, or explicit user corrections; unknown data is shown and correctable, never silently replaced by initial cwd or later state |
| E11 | Save with default environment choice, then explicitly select synthetic configured overrides and confirm | Default file contains no captured environment-variable overrides; opt-in includes only selected overrides, no inherited/runtime environment or value leakage |
| E12 | Confirm a new name into a commented file containing unrelated settings; reopen chooser and restart Harbor | Preserved unrelated settings/comments, new preset persisted and selectable/reloadable, no duplicate publication; its launch reproduces the saved recipe |
| E13 | Change file externally or introduce a name collision after review; use invalid TOML, denied write and an interrupted/failed replacement | Existing content/presets preserved, no force overwrite or success claim, visible error and retained editable draft; valid retry can succeed |
| E14 | Perform save in installed and portable modes, with watcher/manual activity around the write | Only selected file targeted, portable home config untouched, coherent accepted state/no duplicate or stale entries, old sessions unchanged |
| E15 | With guided integration in local pwsh, WSL bash and direct-SSH bash, change project directory, Save/review/confirm, then reopen the preset | Saved environment and Save-time project directory are restored before any selected agent launches; WSL/SSH paths are not treated as native local paths; no global cwd mutation or unsolicited installation |
| E16 | Run a supported agent, let it exit, run ordinary commands and change directory, then Save; repeat after a later supported agent and in separate tabs/environments/SSH hosts | Last recognized agent survives exit/ordinary commands, later supported invocation replaces it only in its scope, and Save uses the current project directory rather than the earlier agent directory; no cross-scope record substitution |
| E17 | Save/reopen recognized pi, Codex, Claude Code and OpenCode invocations with original arguments, including empty/spaced/quoted/backslash/Unicode values and existing resume options | Selected agent receives confirmed argument boundaries/meaning in the saved environment/project; existing resume behavior is honored, no fresh-session override or added resume option |
| E18 | Recognition detects synthetic suspected password/token arguments; review and handle the warning before final Save, or cancel | Handling is visible, credentials are not directly persisted or leaked, silent removal is not represented as exact preservation, confirmed changes remain apparent, and cancellation writes nothing; no perfect-detection claim |
| E19 | Start integration without pre-existing persisted history; contrast supported recognition with arbitrary commands/aliases/wrappers, remote OSC 7, nested environments and activated venv/Conda/Poetry | No prior-history recovery or arbitrary-wrapper recognition is promised, ordinary commands are not replayed, remote metadata triggers no implicit network access, and deferred nesting/virtual-environment activation is not claimed as restored |

## Decisions and Traceability

- R1–R3/E1–E6: #157 plus approved independent-profile/legacy/fail-visible/OSC 7 direction (ADR 0106); inherited literal environment and acceptance rules are spec 0024/ADR 0062; OSC 7 parsing remains ADR 0033.
- R4–R5/E7–E9: owner-selected second-level palette, input-only save, adjacent styled button and internal confirmation (ADR 0107). The owner accepted the recommended back navigation, meaningful review, return/retry, duplicate/empty feedback, repeated-Enter/IME safeguards and lower-layer input gating. Spec 0030/0033 and ADR 0078 retain existing palette availability, editor and exclusive-popup rules; no disabled-reason text is newly required.
- R5–R7/E8–E14: ADR 0108 retains stable review, configured overrides excluded by default with explicit entry selection, confidentiality, and non-destructive selected-file writes. User-requested saving remains distinct from automatic writes prohibited by ADR 0057. Its launch-only capture and initial-directory fallback are superseded, not retained as acceptance.
- R5–R6/R9/E10/E15–E19: ADR 0109 selects environment-aware project presets, recognized agents and guided integration; ADR 0110 and the settled 2026-11-04 handoff govern local pwsh/WSL bash/direct-SSH bash coverage, unknown/correctable context, terminal/environment scoping, last invocation after exit, original arguments/resume semantics and visible suspected-secret handling. ADR 0109's running-only, mandatory-fresh-session and virtual-environment scope are superseded.
- R6/R9/E15–E19: the same handoff explicitly defers multi-layer nesting and venv/Conda/Poetry, requires no pre-integration persisted history, and selects no arbitrary alias/wrapper recognition capability, startup-script feature, concrete capture mechanism or schema. Environment-aware relaunch does not weaken ADR 0033/0106's untrusted-metadata/no-implicit-network boundary.
- R8: #157's scoped evidence, quality-gate and work-package closure requirements. Existing spec acceptance does not prove this new functionality or a fresh runtime run.

## Verification Guidance

Cover complete configuration rejection/publication, safe local URI-to-native conversion without network effects, and actual argv/env/cwd at the child boundary, source provenance across reload and real action-queue execution counts. Exercise integration/recognition → stable capture/correction → name → confirmation → writer → accepted chooser → environment/project/agent relaunch, not only isolated serialization/filter helpers. Inject external-edit/name races, malformed files, write/replacement faults and stale editor/clipboard/IME requests; preserve the on-disk original and draft on failure.

For expanded support, record actual runtime observations in each supported environment: change directory before Save; save with a running and an exited supported agent; run ordinary commands and then a later supported agent; isolate records across tabs/environments/SSH hosts; and inspect argument boundaries and resume behavior after reopening. Exercise absent/unusable integration information and explicit correction rather than accepting an initial-cwd fallback. Use synthetic suspected-secret arguments to verify visible handling without persisting or disclosing credentials. Observe local OSC 7 rejection separately from explicitly requested WSL/SSH launches; no implicit probe or unsolicited installation is allowed.

Windows runtime coverage remains necessary for ConPTY, actual arguments/directories, native IME/focus and confirmation-without-new-window behavior. No new performance/latency threshold, arbitrary URI host allowlist, exact keyboard shortcut, concrete configuration schema spelling, capture algorithm or filesystem locking algorithm is prescribed; implementations must meet the observable contracts without inventing user-visible policy.

No implementation tests or QA are performed by this spec-only task. Existing tickets may describe historical/incomplete scope after this revision; they are not updated here and do not establish execution readiness or acceptance of the expanded requirements.

## Out of Scope

- Direct credential persistence, automatic secret persistence, runtime process/environment cloning or dumping, general command-history capture/replay and arbitrary last-command execution. R9's bounded supported-agent record is the explicit exception, not a history feature.
- Virtual-environment recognition/activation, including venv, Conda and Poetry; multi-layer environment nesting. The first version is limited to local pwsh, WSL bash and direct-SSH bash.
- Guaranteed zero-configuration recognition, arbitrary alias/wrapper recognition, persisted command history from before integration starts, unsolicited remote installation, or an arbitrary manually defined startup-script feature.
- Profile inheritance, a full graphical profile editor, implicit replacement of existing named presets, or changes to ordinary `[shell]` launch/fallback behavior.
- New native save-confirmation windows, conversion of existing paste-confirmation windows, a generic popup stack or new command bus.
- Reconnectable process services, workspace restoration, pane implementation or remote-directory workflows beyond the explicit supported environment-aware presets. Arbitrary remote OSC 7 remains insufficient for native-local cwd inheritance or implicit network access.
- Code implementation, test/QA execution, runtime acceptance, ticket or implementation-plan generation, remote actions, commits or Git-policy changes as part of this specification revision.

# Confirmed Environment-Aware Quick Save and Relaunch

**Ticket ID:** T0003
**Source:** [Spec 0052](../../spec/0052-named-session-profiles-and-quick-save.md), revised 2026-11-04: R5–R7/R9, expanded R2 acceptance and R1/R3/R4 compatibility; E8–E19 plus E1–E6 regressions; [Session Profile Save](../../CONTEXT.md#session-profile-save); ADRs [0106](../../adr/0106-independent-session-profiles-and-explicit-osc7-launch-policy.md), [0107](../../adr/0107-shared-palette-profile-flow-and-in-window-confirmation.md), [0108](../../adr/0108-explicit-session-preset-snapshots-and-safe-configuration-writes.md), [0109](../../adr/0109-environment-aware-project-presets-and-recognized-agents.md), [0110](../../adr/0110-last-recognized-agent-launch-with-original-arguments.md).
**Status:** Todo
**Allocation:** Owner's refine handoff and [relationship README](./README.md): T0001/T0002 files stay unchanged; this ticket owns all expanded capture/schema/validation/publication/relaunch requirements, including necessary work in shared production seams.

## Goal

A user saves a reusable project preset from local pwsh, WSL bash or direct-SSH bash: capture the identified environment and actual current project directory when Save begins, review/correct a stable draft, explicitly persist safely, and reopen that environment and directory before launching any selected supported agent with confirmed original arguments.

Remember the last recognized pi, Codex, Claude Code or OpenCode invocation even after it exits. Unknown/unusable environment or directory recognition is shown and correctable, not silently replaced by an initial directory. Keep capture → review → safe persistence/accepted catalog → environment/agent relaunch cohesive: UI-only saving, serialization alone or queue insertion is not delivery (R2/R5–R7/R9).

## Affected Surfaces

- Source-terminal provenance and guided shell integration/recognition; terminal-and-execution-environment-scoped agent records and capture in `src/profile_save.rs` and existing session/Host boundaries (R6/R9).
- Existing configuration schema/serialization/validation in `crates/harbor-config/src/lib.rs` and config writer; extend accepted preset data and complete-candidate reload/publication for identified environment, project directory and confirmed agent arguments (R2/R6/R7/R9).
- Application-owned save draft, naming/review/correction and final confirmation in `crates/harbor-app/src/quick_save.rs`; menu/palette commands, exclusive-popup/editor/IME/focus integration (R5).
- Host/factory/PTY relaunch boundaries including `src/profile_launch.rs`, `src/shell.rs`, `src/tab_coordinator.rs` and platform argument handling. Extend beyond native-local launch to saved environment → project directory → selected agent, preserving original profile/legacy behavior (R1–R3/R9).
- Selected-path writer, comment-preserving edits, accepted catalog/future launch state and watcher/manual interleaving; focused integration and failure-injection tests (R2/R7/R8).

These are existing integration seams, not a prescribed module layout. T0001's baseline fields/provenance are foundations, not a restriction to UI/persistence work or evidence that expanded requirements are already implemented.

## Direction and Boundaries

### Capture and relaunch

Use the source environment and current directory at Save initiation, not the original process directory, earlier agent invocation directory, latest mutable profile definition or stale/unknown metadata. Retain source provenance and recognized information across configuration reloads. After capture, later tab/directory/agent-record/config changes cannot silently mutate the reviewed draft; explicit user corrections remain visible (R6/E10).

Guided integration is accepted for local pwsh, WSL bash and direct-SSH bash, potentially requiring WSL/SSH-side cooperation. No concrete protocol, capture mechanism, recognition algorithm, installation procedure or schema is selected by this ticket. This does not authorize unsolicited local/remote configuration changes or remote installs (R6; ADR 0110).

Retain the last recognized supported-agent invocation with original argv in its terminal/environment scope. Exit and ordinary commands do not erase/replace it; a later recognized supported invocation replaces it only in that scope, never across tabs or SSH hosts. Reopen the saved environment and project directory before the selected agent; preserve argument boundaries/meaning and original resume options without adding resume options or forcing a fresh session (R9).

Keep R3's local named-launch directory precedence unchanged and separate. A WSL/SSH project path is interpreted in its saved environment, not native-local validation; arbitrary remote OSC 7 is neither local cwd evidence nor permission for network probes. Reuse existing application-command, Host/factory and platform quoting boundaries, not UI-constructed shell command strings or an arbitrary script executor (R1–R3/R6/R9).

### Review and confidentiality

Use input-only naming with the Save… button to the right of the name input, same row inside the panel, matching input height/corners/material and retaining hover/press feedback. Button and Enter validate then advance, never write. Confirmation is an internal overlay in the existing window with Return to edit and explicit final Save (R5).

Review name, identified environment, save-time directory, launch program/arguments, recognized agent invocation/arguments if present, and configured override inclusion. Recognition limitations must be visible and correctable. Suspected password/token arguments require visible user handling before persistence: credentials are not directly persisted, silent deletion cannot be called an exact copy, confirmed changes remain apparent, and no perfect-detection claim is made. Avoid unredacted credential/environment-value review or diagnostic dumps (R5/R9).

Configured environment overrides remain excluded by default. Only deliberately selected existing configured entries may be included after confirmation they contain no passwords/tokens. This is independent of agent-argument handling and deferred virtual-environment capture; never scrape runtime/inherited environment (R6/E11).

### Persistence and compatibility

Only final Save authorizes adding a new preset to the startup-selected configuration. Preserve unrelated settings/comments and existing presets; reject conflicts, invalid contents, duplicate names and write faults visibly while retaining editable input. Protect against partial/truncated writes; success requires both durable selected-file presence and coherent accepted publication, not just one outcome (R7).

Expanded fields use the same complete-candidate acceptance contract: invalid presets cannot partially install catalog/live fields/future policy; accepted removals do not merge old generations. Preserve independent existing profiles, ordinary `[shell]` behavior, local named-launch precedence, literal child-only environment rules, existing children and selected-file installed/portable isolation. Keep startup recovery distinct from strict live acceptance (R1–R3/R7).

## Dependencies and Coordination

- **Blocked by:** T0001's baseline profile schema/validation/publication and retained source launch provenance. Its initial-directory facts are not a current-directory capture fallback.
- **Blocks:** T0004's combined native capture/save/chooser/environment-agent relaunch evidence.
- **Parallel with:** T0002 after T0001. Verify accepted catalog publication and Host relaunch independently of the final chooser; T0004 verifies the combined chooser interaction.
- **Coordination:** Coordinate command/palette/popup/workspace/Host and catalog edits with T0002, without a blocker solely for shared files. Extend needed shared schema/Host seams here, not by silently enlarging the frozen T0001/T0002 ticket files. Preserve existing native paste confirmation, exclusive-popup retirement and typed action admission.

## Acceptance

### Capture, scope and relaunch

- [ ] **R6/E10/E15:** In each of local pwsh, WSL bash and direct-SSH bash with guided integration, change directory before Save. Review shows that environment and Save-time project directory. WSL/SSH paths are not treated as native Windows paths; no initial-cwd or earlier-agent-directory substitution occurs.
- [ ] **R5/R6/E10:** Missing, stale or unusable environment/directory recognition is visibly unknown/unusable and user-correctable, not presented as confirmed current data. Return to edit retains the name/draft and permits corrections. Later tab, directory, agent-record and configuration changes leave the reviewed snapshot/corrections stable.
- [ ] **R6/R9/E16:** Save with a running supported agent and after it exits; ordinary commands preserve the last recognized invocation. A later supported-agent invocation replaces it only in the current terminal/environment. Separate tabs, environments and SSH hosts do not substitute each other's records. After changing cwd, Save captures current cwd rather than that of the earlier invocation.
- [ ] **R9/E15/E17:** Through the accepted catalog and Host launch contract, reopen saved local pwsh, WSL bash and direct-SSH bash presets into their environment/project before any selected agent launches. For pi, Codex, Claude Code and OpenCode, actual launches preserve confirmed original empty/spaced/quoted/backslash/Unicode arguments across environment boundaries. Original resume options remain effective; no forced fresh session or added resume option.
- [ ] **R2/R9/E6/E15:** Extended schema/validation/publication accepts coherent environment-aware presets and rejects invalid complete candidates without partially publishing profiles, live fields or future launch policy. Accepted add/edit/remove and save-triggered reloads remain coherent; remote directories are not native-local validated and validation/metadata causes no implicit network probe.
- [ ] **R1–R4/E1–E6:** Existing independent profiles and ordinary `[shell]` behavior remain compatible. Preserve centralized argument quoting, literal child-only overrides, case-conflict rejection, empty/removal semantics, protected TERM and secret-free diagnostics. Keep config-relative local paths, R3 local precedence, visible named-launch/directory failure without substitution, current-profile revalidation and unchanged existing children/global cwd/environment. Do not substitute malformed-profile launch recipes or change startup recovery outside new fields.

### Naming, review and input lifetime

- [ ] **R5/E8:** Menu and palette expose the same save flow. Input-only naming has the styled right-hand Save… button with hover/press feedback; Enter and click both validate/advance without writing. Empty/duplicate names have nearby feedback, disabled advance and no keyboard bypass.
- [ ] **R5/E9/E10:** Internal confirmation presents the required name/environment/directory/program/arguments/agent and override-inclusion review, with limitations/corrections explicit. Return retains input; cancel writes nothing; failures retain correction/retry state. No new native window is created.
- [ ] **R5/E9:** Held/repeated advance Enter cannot also confirm Save; IME candidate confirmation is not submission. Confirmation-layer actions reach neither lower editor nor PTY. Retired editors, late clipboard/IME and stale queued requests cannot edit/commit a replacement flow. Dismissal restores eligible terminal focus without consumed-input leakage or resurrected popup state.
- [ ] **R5/R9/E18:** With synthetic suspected credential arguments, visible handling occurs before persistence or the user cancels. Credentials do not appear in the saved invocation or diagnostic/evidence dumps; any confirmed modification stays visible and is not described as exact preservation. Cancellation writes nothing; no perfect secret-detection claim.
- [ ] **R6/E11:** Default saving captures no environment-variable overrides. Explicit selection includes only elected existing configured entries after the no-password/token confirmation; no inherited/runtime environment capture or value disclosure. This policy remains distinct from argument warnings and virtual-environment activation.

### Safe commit and verification

- [ ] **R7/E12/E14:** Only final confirmation writes a new preset into the selected installed/home or portable/executable-adjacent file. Preserve comments, unrelated settings and existing presets; never fall back to another writable file or portable home config. Successful save is durable, accepted, available to the shared chooser contract and reloadable after restart with the saved relaunch recipe.
- [ ] **R7/E13:** External changes, invalid TOML, a duplicate discovered at commit, denied writes and interrupted/failed replacements produce visible honest errors and retained editable input. Existing content/presets are protected from overwrite/loss and partial/truncated writes; commit-time conflicts are rechecked and valid retry can succeed.
- [ ] **R2/R7/E12/E14:** Watcher/manual activity around save cannot duplicate entries, publish stale generations or partially accept unrelated candidate fields. Report incomplete disk/in-memory outcomes honestly, retaining the draft rather than claiming complete success. No automatic startup/watch/reload write behavior changes.
- [ ] **R6/R9/E19:** Start integration without pre-existing history; do not promise pre-integration recovery or arbitrary alias/wrapper recognition. Ordinary commands are not replayed; remote OSC 7 triggers no implicit network access. Multi-layer nesting and venv/Conda/Poetry recognition/activation are not claimed restored, and no unsolicited installation occurs.
- [ ] **R8; spec Verification Guidance:** Focused tests exercise recognition → stable capture/correction → naming/confirmation → selected-file writer → accepted catalog → Host environment/project/agent relaunch, with synthetic data, real action execution counts and file/input fault cases. Record exact commands/results and native omissions for T0004; isolated helpers or queue insertion do not establish delivery. Combined final chooser/native interaction belongs to T0004.

## Out of Scope

Virtual-environment detection/activation (venv/Conda/Poetry); multi-layer nesting; arbitrary command/history replay or manually defined startup scripts; arbitrary alias/wrapper or pre-integration history recovery guarantees; runtime process/environment cloning/dumps; direct credential persistence or perfect secret detection; unsolicited local/remote setup or remote installation (R6/R9/E19).

Profile inheritance, full graphical profile editing, overwriting existing named presets, changing legacy fallback, parser-driven filesystem effects/global chdir, new native save-confirmation windows, generic popup stacks/second command buses, automatic configuration writes/path migration and unrelated paste-confirmation changes remain excluded (R1–R7).

This revision writes delivery obligations only. No implementation, tests/runtime QA, source-record changes or new plan/ticket are performed. Material semantic gaps return to requirements refinement; unselected implementation mechanisms are not permission to invent product policy.

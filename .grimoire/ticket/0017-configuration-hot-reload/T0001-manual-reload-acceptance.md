# Accept the Existing Manual and Font Reload Slice

**Ticket ID:** T0001
**Source:** [Spec 0024 R1/R6](../../spec/0024-watched-reload-and-launch-settings.md), [spec 0023](../../spec/0023-atomic-manual-configuration-reload.md), [#172](https://github.com/wangjq4214/harbor/issues/172), [PR #201](https://github.com/wangjq4214/harbor/pull/201).
**Status:** Done — automated checks and scoped Windows smoke passed; remaining manual acceptance confirmed by the user.

## Goal

Establish the actual delivered scope of the existing manual transaction, repair confirmed in-scope failures, and record Windows acceptance (local-only evidence permitted by the execution contract below) rather than treating source/tests as a pass.

## Affected Surfaces

Existing coordinator/factory, strict config loader, input dispatch, terminal palette/font/resize resources, manual transaction tests, and user/status/evidence documentation. ADR 0059–0061 and spec 0023 remain authoritative.

## Approach

Use the current manual implementation instead of reconstructing the original colors-only slice. Locate prior evidence or produce the missing cases. Verify actual application behavior in addition to failure-injection and parser/model tests. Record scope discrepancies for authorized remote reconciliation.

## Dependencies and Coordination

- **Blocked by:** No local ticket; requires the existing PR #201 transaction code or its successor.
- **Provides:** Accepted manual/font baseline for T0004; this dependency is satisfied, while T0004 still owns integrated package acceptance.
- Coordinate any fixes in shared files with T0002/T0003; evidence work alone is not their blocker.

## Acceptance

- [x] Spec 0023 E1–E11 have explicit results, including active/inactive/alternate sessions, OSC reset precedence, consumed chords, startup fallback, new-session shell and restart-required clipboard reporting.
- [x] Real Windows cmd/PowerShell, nvim, font changes, continued prompt/input, selection/copy and supported IME paths have reproducible results; exclusions remain explicit.
- [x] Preparation failure, later-session resize failure and failed compensation retain prior accepted settings, resume readers and report honest outcomes; retry is covered.
- [x] Applicable five quality gates from spec 0024 are executed or marked NOT RUN/BLOCKED with reasons.
- [x] Evidence records revision/environment and expected/observed behavior locally. The user waived the retrievable-file requirement for this execution; do not change ignore policy or force-add artifacts.
- [x] #172's font exclusion, PR scope and local status discrepancies are reconciled through authorized updates, or explicitly reported pending without claiming issue closure.
- [x] No broad compatibility or #155-complete claim is made from this slice.

## Execution Result — 2026-10-15

The user explicitly authorized local-only evidence and verified-status updates, waived a committed/retrievable evidence artifact, and prohibited `.gitignore` changes. No ignore-policy change, force-add, commit or remote mutation was made. Production code was unchanged; this is an acceptance/status update, not a reimplementation.

**Verified baseline:** `bb307d812a27919291792c32a58038bf94f3ba07`; Windows 26300.9457, bundled ConPTY 1.24.260710001, Vulkan on RX 7900 XT, PowerShell 7.6.6 and nvim 0.12.5. Five gates passed; workspace aggregate 2449 passed / 0 failed / 10 ignored. The 8 focused application reload tests passed, including preparation rejection, native later-session failure, failed compensation, reader resume and retry.

**Verified desktop scope:** cmd and PowerShell pending-command/prompt continuity; unchanged existing cmd PIDs; Consolas/Cascadia Mono and repeated size changes; active/inactive/new tabs; nvim redraw and return to retained primary history; invalid-font rejection and recovery; R-to-G reload rebind; new-session shell and restart-required clipboard feedback; copying retained text after font changes. Raw VT ReadFile probes after real ConPTY received exactly `7871` for x/q during successful and rejected reloads, with no reload-chord bytes.

**Spec 0023 case results:** E1/E2/E3/E5/E8 have automated PASS plus the bounded desktop observations above; E4/E6/E7/E10/E11 have automated PASS (E7 includes native reader/PTY fixtures); E9 has deterministic binding lifecycle PASS plus scoped native consumed-chord PASS. These are method-specific results, not complete E2E acceptance of every clause.

**Manual acceptance confirmed by the user:** In the follow-up conversation, the user confirmed that the scenarios this run had left unverified were already manually verified in the previous round and explicitly requested T0001 completion. This closes the remaining ticket acceptance for IME, physical Backspace, desktop OSC/reset and startup fallback, held-key rebinding, and the scoped backdrop/DPI checks. These results are user-reported manual verification, not newly executed agent tests; the earlier automation limitations remain part of the record. The completion decision applies to the unchanged implementation baseline above (only documentation changed in this run). No additional prior-run environment or measurements are invented, and no broad compatibility claim follows.

**Local records:** `docs/verification/manual-configuration-reload.md` contains E1–E11 expected/observed results and reproduction steps; `target/manual-reload-acceptance/` contains logs, screenshots, process identities and byte probes. These paths deliberately remain local, not required delivery artifacts. Temporary test config and owned processes were removed after use.

**Remote reconciliation pending:** #172 still excludes fonts and PR #201 needs scope/evidence alignment; no closure is claimed. T0004 remains responsible for integrated package acceptance; neither #155 nor broad compatibility is complete.

## Out of Scope

Implementing watcher or environment/directory support; closing the full package; unrelated runtime features.

# Atomic Manual Configuration Reload

**Spec ID:** 0023
**Status:** Draft
**Date:** 2026-10-14
**Sources:** [Issue #172](https://github.com/wangjq4214/harbor/issues/172), the conversation contract below, [ADR 0059](../adr/0059-atomic-manual-reload-and-terminal-only-fonts.md), and [ADR 0060](../adr/0060-font-reload-uses-normal-bounded-resize.md).

## Source Authority and Conversation Contract

- **C1 — Accepted recommendations:** The user accepted the proposed strict manual reload, existing command dispatch, `app.reload-configuration` with default `Ctrl+Shift+R`, non-modal application feedback plus logging, OSC RGB override preservation with resets to the latest accepted baseline, new-session-only shell changes, and restart-required clipboard policy.
- **C2 — Font scope expansion:** The user required terminal font changes without application or PTY restart and without affecting external UI fonts. Replacing rendering resources must not recreate sessions or clear terminal buffers.
- **C3 — Bounded retention clarification:** After font size changes, use normal geometry and rendering behavior and retain the newest content. Ordinary history eviction or rectangular-screen clipping is not a reason to reject reload. This qualifies C2's earlier unqualified content-preservation wording.
- **C4 — Compensating-resize failure exception:** During implementation planning the user accepted an explicit recovery-failed outcome if a later-session PTY resize fails and restoring an already-resized PTY also fails. Keep the previous accepted configuration, do not report success, resume every reader, attempt geometry reconciliation, and identify unrecovered sessions. This narrow exception does not permit partial configuration publication or weaken ordinary rejection/rollback requirements. See [ADR 0061](../adr/0061-manual-reload-compensation-failure.md).

Issue #172 still excludes font/resource transitions and leaves font work as follow-up. C2 explicitly expands that part of the issue for this contract; C3 and ADR 0060 qualify ADR 0059's retention wording. Other issue exclusions and evidence obligations remain applicable. This spec does not modify the GitHub issue or claim that its published scope has already been reconciled.
**Follow-up scope:** [Spec 0024](./0024-watched-reload-and-launch-settings.md) records the subsequently approved default-on watcher and new-session environment/launch-directory extension for #155. Those features remain outside this initial manual slice, but are not deferred from the combined package. The manual/font/OSC/failure contract below remains applicable.

Existing source-backed contracts are retained where this extension does not change them: [structured application bindings](../adr/0041-structured-keybinding-configuration.md), [command precedence](../adr/0040-application-command-dispatch-and-keybinding-precedence.md), [resize/reflow](./0014-content-preserving-resize-reflow.md), [the reader barrier](../adr/0043-acknowledged-pty-resize-barrier.md), [ConPTY live-primary compatibility](../adr/0052-conpty-live-primary-producer-geometry-and-styled-tail-clipping.md), [terminal/application appearance separation](../adr/0058-terminal-themes-separate-from-application-appearance.md), and [validation policy](../../docs/validation.md).

## Requirements

### R1 — Manual Command and Input Availability

Register the implemented typed Reload Configuration operation as `app.reload-configuration`, with display metadata and default `Ctrl+Shift+R`. It uses the existing application registry, shortcut/action outcome, FIFO dispatcher, and application-owned executor; it must not require a command palette or introduce a second command bus.

The reload binding uses the existing structured TOML format:

```toml
[keybindings.app.reload-configuration]
bindings = [{ modifiers = ["ctrl", "shift"], key = "r" }]
```

Omission keeps registry defaults, configured records replace defaults, and `bindings = []` unbinds the command, just as for existing commands. Do not introduce an unconfigurable rescue chord.

Reload is an application operation, not an alternate-screen terminal key. Preserve existing cross-window input gates and supported command availability. A consumed reload chord must not reach the PTY even if reload fails. Binding replacement must not retroactively pass an already-consumed chord's repeat/release handling to terminal input. Preserve existing conditional pass-through for copy-or-interrupt and alternate-screen history commands, and existing IME behavior.

### R2 — Complete, Strict Candidate Validation

Read the current `~/.harbor/config.toml` when manually invoked. A missing or unreadable file, malformed TOML, invalid recognized field, invalid color, malformed binding shape, unknown command ID, unsupported key/modifier, repeated modifier/binding, or cross-command chord conflict rejects the complete reload.

Validate all recognized candidate settings, not only changed live fields. In particular, invalid font, shell, or clipboard field values cannot be silently replaced with startup defaults while colors or bindings are accepted. Semantic keybinding validation remains application-owned and includes registry defaults when checking conflicts.

Each candidate is a complete settings document under the existing defaults/override schema, not an incremental patch over the last accepted settings. Removing a valid optional override therefore restores its documented default; omission is not an error. Unknown ordinary configuration fields retain visible warning behavior rather than silently becoming supported settings. The superseded string-array keybinding format remains invalid.

These rules concern acceptance of malformed configuration, not a new font-discovery policy. Keep the existing terminal font selection and glyph fallback behavior documented in CONTEXT and implemented by `harbor-text`: an unavailable configured family warns and uses a usable system primary if available. Failure to obtain usable font resources, valid metrics, or GPU resources rejects reload. Do not conflate that established font-selection fallback with accepting malformed TOML through startup fallback.

### R3 — Coherent Acceptance and Failure

The application thread owns publication of the accepted configuration. Complete validation and supported resource/model preparation before publishing a new generation. Existing applicable sessions, UI bindings, and future-instance factories must not finish reload on different accepted generations.

A rejected candidate leaves the previously accepted colors, OSC baseline/overrides, binding table, terminal font resources/metrics, and future-instance configuration effective. It must not partially accept colors when font preparation fails or update some tabs while leaving others on old resources. Failure feedback must not claim success.

Font-driven grid changes must keep terminal model, PTY geometry, renderer metrics, and input/IME coordinates coherent. Actual resize/barrier failures cannot be ignored and reported as a successful font reload. Preserve the existing ordered reader barrier and resume it on every completion/error path; do not lose or duplicate PTY bytes.

This is configuration atomicity, not a promise to freeze or roll back normal child-process execution and terminal output. A per-renderer replacement or a sequence of independent per-session resize calls is not, on its own, proof of all-session acceptance. The implementation must demonstrate its multi-session failure behavior rather than weaken this contract through an undocumented partial-success exception.

**Recovery-failed exception (C4):** If a PTY resize has succeeded but compensation after another session's resize failure also fails, report both operations and affected sessions explicitly. Retain the prior accepted configuration/resources and resume every paused reader. Attempt to reconcile model/render/input geometry with the last successfully established PTY geometry; do not assert that old geometry was restored when it was not. A session that remains unrecovered must be identified as such. This is a failed operation, never a partial accepted generation or success. Controlled tests must cover this double-failure path as well as ordinary successful compensation.

### R4 — Supported Application Policies

| Setting | Required application policy | Source |
| --- | --- | --- |
| Terminal default/ANSI colors, cursor color, selection color | Apply live to all relevant existing sessions and future instances, subject to R6's OSC precedence | Issue #172, C1, ADR 0059 |
| Application keybindings | Publish the complete resolved live table without clearing terminal/session state | Issue #172, C1 |
| Terminal font family and size | Apply live to existing and future terminals; external UI fonts remain independent | C2, ADR 0059 |
| Shell program and arguments | Update new-session configuration only; never restart or relaunch an existing child | C1, ADR 0059 |
| Clipboard policy | Do not apply live in this scope; report a changed value as restart-required | C1, ADR 0059, current configuration example |

Classify changed settings accurately. A legitimate restart-required setting may coexist with an accepted live subset, but this is not permission to accept malformed settings. A successful reload must not imply that a changed clipboard policy is already effective. Unknown keys must be reported as unknown/ignored, not falsely represented as supported after restart.

### R5 — Terminal-Only Font Changes and Normal Bounded Resize

Replace terminal font/render resources while keeping the existing application, terminal session, PTY, and running child process. Do not implement reload by clearing buffers or constructing fresh terminal sessions. Do not replace widget/chrome/dialog font resources or change their font family, size, or text metrics because terminal font settings changed.

Use the new primary terminal font metrics for terminal rendering and grid calculation. If geometry changes, use the existing buffer-specific resize policy and notify the existing PTY of its new dimensions; if metrics imply unchanged grid dimensions, do not manufacture a grid resize merely to redraw.

- Primary content reflows at the new width under the established main/history/ConPTY policies. Retain the newest content within the existing physical-row capacity, allowing normal oldest-history eviction and existing tail-trimming rules.
- Active or parked alternate screens retain the existing rectangular policy, including boundary clipping, wide-cell repair, and application-driven redraw. Do not text-reflow them or add alternate-screen scrollback.
- When alternate mode is active, resize the saved primary state under its own policy so leaving alternate mode restores the current geometry, not a cleared or stale primary screen.
- Preserve cursor, review, selection, and hyperlink meaning according to existing resize/anchor validity rules. Invalidation caused by ordinary eviction/clipping is allowed; unrelated state resets are not.
- Apply updated fonts and palette state to inactive sessions as well as the active one. The next render of an inactive session must not replay stale old-font/old-palette GPU data. New terminals use the accepted font settings and matching metrics/resources.
- Invalidate or replace font-dependent glyph/sequence caches and retained rendering projections as needed so old glyph images, underline/box geometry, cursor placement, selection bounds, or input/IME coordinates do not persist with new metrics.

Do not reject an otherwise valid reload solely because normal resize evicts older history or clips cells. Do not add unlimited history, off-grid backups, a second coordinate model, or a special lossless font-resize policy to satisfy this contract. Actual preparation failures and existing resize validity failures remain errors under R3.

### R6 — Configuration Baseline and OSC Precedence

Each terminal session has the latest accepted configuration palette as its reset baseline and its own explicit OSC RGB overrides. At startup the baseline is the startup palette. Successful reload advances the baseline; failure does not.

- Unoverridden default foreground/background/cursor and indexed entries immediately use the new baseline. Selection uses its configured color; it is not an OSC default/indexed override slot.
- Preserve explicit OSC 10/11/12 RGB overrides and OSC 4 overrides for indexes 0-255 across successful reload. An explicitly set RGB value equal to the old baseline is still an override, not evidence that no override exists.
- Alpha follows the newly accepted configuration baseline. OSC overlays RGB, not a stale RGBA copy; ordinary OSC sets continue to preserve the active alpha.
- OSC 110/111/112 clear the corresponding default-color override and restore the complete latest baseline RGBA. OSC 104 clears/restores selected indexed overrides, or all indexed overrides for its empty payload, without resetting default-color or selection slots.
- Palette state and overrides are shared across primary/alternate buffers of one session and isolated between sessions. New sessions inherit the accepted baseline, not another session's OSC overrides.
- Already-written cells retaining default/indexed color semantics reflect the active palette after reload. Explicit truecolor cell values are not rewritten into theme colors.
- Preserve the existing bounded OSC syntax, malformed-request rejection, query reply format/framing, capacity, and RIS/DECSTR/SGR behavior. This extension changes reload/reset-baseline precedence, not the wire protocol subset.

Terminal colors and fonts must not recolor application chrome, select application day/night mode, or change the independent whole-window compositor tint.

### R7 — Actionable Results and Honest Documentation

Provide non-modal application-visible reload results and corresponding diagnostics in logs. Failure identifies the configuration path and relevant parse location, setting/binding path, or resource/resize operation where available, and states that the previously accepted configuration remains effective. Diagnostics are application feedback, not shell text or PTY injection.

Success distinguishes applied live changes, new-session-only changes, and restart-required changes. Surface relevant ignored-setting and font-selection warnings without calling malformed configuration a success. Feedback must not require the deferred command palette or disturb the established input-gate policy.

Keep user-facing defaults, the reload binding, supported setting policies, OSC reset behavior, and known exclusions accurate when implementation is delivered. Do not mark runtime/protocol status complete on source or parser tests alone.

### R8 — Startup Compatibility and Delivery Evidence

Preserve the existing startup rules: document/read failure uses complete defaults; ordinary scalar failure is local; invalid palette restores the whole palette; invalid keybindings restore all default bindings without discarding valid unrelated settings. Adding reload does not replace those behaviors with strict live policy.

Delivery requires focused tests, reproducible Windows runtime evidence, and the quality/evidence gates in issue #172 and `docs/validation.md`. The evidence must distinguish this expanded font-inclusive scope from the issue's currently published colors/bindings-only scope.

## Solution

Extend the application-owned command/configuration path rather than widget-library HMR. Treat a reload candidate and its prepared supported resources as distinct from accepted live state. The application owns the acceptance boundary and exposes the accepted binding state to UI shortcut construction instead of leaving it as an immutable startup-only root input. Preserve existing terminal/session identity while installing accepted palette and terminal font/render changes.

Use the existing ownership split: `harbor-config` parses setting structure, `harbor-app` validates command semantics and presents shortcuts/results, the binary application coordinates acceptance and future factories, `harbor-text` owns terminal font selection/metrics, and `harbor-terminal` owns content, resize/PTY coordination, and terminal rendering resources. The exact internal containers and multi-session commit mechanics are not prescribed; their externally observable guarantees are.

### Seams

| Seam | Connects | Expects | Provides |
| --- | --- | --- | --- |
| Candidate settings | Configuration file / `harbor-config` → application coordinator | A freshly read complete document; typed settings, raw binding records, and diagnostics without treating fallback as live success | R2 validation evidence and a candidate that cannot silently replace malformed values with defaults |
| Command resolution | Raw bindings / registry → application binding validation | Stable IDs, structured records, registry defaults, complete conflict checking | A complete resolved table or rejection; the same command identity used by manual invocation and future palette discovery |
| Application acceptance | Coordinator → existing sessions, UI state, and `TerminalTabFactory` | Completely validated candidate and prepared supported changes; failure propagated from every affected instance | One coherent accepted configuration, or previously accepted configuration remaining effective under R3 |
| Terminal font resources | `harbor-text` / terminal renderer → application preparation | Usable terminal font selection, primary metrics, and validated GPU/cache resources on their owning thread | Replacement terminal resources independent of widget UI fonts, with failures observable before acceptance |
| Geometry and PTY | Terminal model/bridge ↔ PTY reader/control | New metrics and allocated terminal region, buffer-specific prepared resize, ordered reader barrier, actual PTY resize result | Consistent grid/render/input geometry, exactly-once byte delivery, reader resume, and existing bounded retention semantics |
| Palette projection | Session baseline/OSC state → terminal update/renderer | Explicit RGB override ownership and latest accepted baseline | Consistent queries, cell resolution, clears, cursor/selection colors, and invalidated retained GPU state across active/inactive/alternate views |
| User feedback | Application executor → application-visible result and diagnostics | Actual outcome and setting application classification | Non-modal actionable failure/success feedback without PTY injection or false live/restart claims |

Current touchpoints are [config parsing](../../crates/harbor-config/src/lib.rs), [command resolution](../../crates/harbor-app/src/command.rs), [root binding inputs](../../crates/harbor-app/src/workspace.rs), [application execution](../../src/shell.rs), [tab factories](../../src/tab_coordinator.rs), [font policy](../../crates/harbor-text/src/font.rs), [font selection fallback](../../crates/harbor-text/src/backend/dwrite.rs), [terminal resize](../../crates/harbor-terminal/src/lib.rs), [renderer font replacement](../../crates/harbor-terminal/src/render/pipeline.rs), and [palette baseline storage](../../crates/harbor-terminal/src/screen/default_colors.rs). Existing primitives are starting surfaces, not evidence that application reload already exists.

## End-to-End Tests

### E1 — Valid Colors and Bindings Without PTY Restart

- **Given:** Existing primary-screen sessions, an inactive tab, and a byte-recording PTY fixture; the settings file changes foreground, background, cursor/selection colors and a command binding.
- **When:** The user invokes `Ctrl+Shift+R`, then exercises the old and new bindings and activates the inactive tab.
- **Then:** All sessions observe the accepted color policy, the replacement binding works, the replaced chord no longer invokes its old command, unchanged/default and empty-unbound commands behave correctly, child/session identities remain the same, and the consumed application/reload chords emit no terminal bytes. The user sees success feedback.

### E2 — Invalid Candidate Retains the Previous Generation

- **Given:** A known accepted non-default palette, font, binding table, OSC override, and factory settings.
- **When:** Reload is invoked separately with malformed TOML, missing/unreadable file, invalid color, malformed/unknown/conflicting binding, invalid font size/family shape, invalid shell shape, or invalid clipboard policy, including documents mixing valid live changes with one invalid field.
- **Then:** The complete reload fails with actionable feedback; every existing session and subsequently created instance still uses the previous accepted applicable configuration. No default fallback or valid subset is committed; ongoing terminal output remains possible.

### E3 — Font Family/Size Changes With Independent UI Fonts

- **Given:** Known terminal font metrics, primary/history content with identifiable newest markers, independent widget/chrome/dialog font metrics, and a still-running shell with pending input.
- **When:** A valid candidate changes terminal font family and size, followed by further typing and another reload that restores the prior size.
- **Then:** Terminal glyphs and metrics change, affected PTYs receive matching grid sizes, newest retained text/prompt/input remains usable, and the shell/session is not recreated. UI fonts and compositor tint are unchanged. Cursor, decoration, selection, and input/IME coordinates match the new terminal geometry rather than stale cached metrics.

### E4 — Normal Capacity Eviction Is Accepted

- **Given:** Primary history near its existing capacity with old and recent markers and selection/review anchors, including a case where the new font size reduces columns.
- **When:** A valid font reload causes reflow to exceed physical-row capacity.
- **Then:** Reload succeeds using the normal resize policy, oldest overflow is evicted, newest content remains retained, surviving anchors remain meaningful and evicted anchors invalidate/fall back normally. The operation does not reset buffers, allocate an unlimited backup, or fail merely because old rows were evicted.

### E5 — Inactive and Alternate-Screen Font Reload

- **Given:** Multiple sessions, including an inactive session and a persistent alternate-screen application with saved primary history and identifiable content.
- **When:** A valid font/color reload changes grid dimensions; tabs are activated, the full-screen application redraws, alternate mode exits, and a new tab is created.
- **Then:** Every rendered instance uses the accepted font/palette, the alternate surface follows normal rectangular clipping/redraw, the saved primary restores at current geometry with its normally retained content, and the new tab uses matching accepted metrics/resources without inheriting another session's OSC overrides.

### E6 — OSC Overrides Survive and Reset to the New Baseline

- **Given:** Two sessions with different OSC default/indexed RGB overrides, including an explicit override equal to the old baseline, and content using default, ANSI/indexed, and truecolor colors.
- **When:** Reload changes baseline RGB and alpha, OSC queries are sent, and corresponding default/indexed reset commands are exercised in primary and alternate modes.
- **Then:** Explicit RGB overrides survive with new baseline alpha; unoverridden slots and selection use new configuration; query/display RGB agree; reset clears only relevant overrides and returns to latest baseline RGBA. The second session remains isolated, new sessions start without overrides, and truecolor cell values are unchanged. A failed reload does not alter any baseline/override.

### E7 — Preparation and Resize Failure Cannot Become Partial Success

- **Given:** Multiple sessions and a candidate changing colors, bindings, fonts, and future shell settings, with controllable failure in font acquisition, metrics/GPU preparation, reader barrier, or PTY resize, including failure associated with a later affected session.
- **When:** Reload is attempted and the failure is triggered.
- **Then:** The operation does not publish a partial accepted generation or report success. Previously accepted settings/resources remain effective, model/PTY/render geometry is coherent, pending damage is not lost, and every paused reader resumes. A subsequent valid reload can succeed. Per-session success alone is not sufficient evidence for this case.
- **Compensation failure variant (C4):** Fail a later session's PTY resize and then fail restoring an earlier successfully resized PTY. The reload still fails without publishing any candidate settings. Every reader resumes; feedback identifies both failures and any unrecovered session. Geometry reconciliation is attempted under the previous accepted font policy; old geometry is not falsely claimed restored.

### E8 — Mixed Live, New-Session, and Restart-Required Changes

- **Given:** Existing child processes and a candidate with valid live settings, a changed shell program/arguments, and a changed clipboard policy.
- **When:** Reload succeeds and a new tab is opened.
- **Then:** Existing children continue unchanged, the new tab uses the accepted shell settings, effective clipboard policy remains the previously applied policy, and the result explicitly distinguishes live/new-session/restart-required changes. No running process is restarted to implement a setting.

### E9 — Reload Rebind and Existing Input Semantics

- **Given:** A candidate rebinds the reload command itself and changes another application chord, with deterministic key-down/repeat/key-up capture and existing input-gate fixtures.
- **When:** The old reload chord is pressed/held/released across acceptance, then the new binding and ordinary terminal/IME input are exercised; repeat with a failed candidate and in alternate mode.
- **Then:** The consumed invocation does not leak terminal encoding across the table switch or failure, the new binding is available according to existing gates, and conditional copy/scroll pass-through and normal terminal/IME routing remain intact. Explicitly unbinding reload removes its shortcut under normal binding rules.

### E10 — Startup Fallback Remains Separate

- **Given:** Separate startup documents containing an invalid scalar, invalid color, invalid binding set, malformed TOML, or missing file, plus valid unrelated overrides where applicable.
- **When:** Harbor starts, rather than reloading a live application.
- **Then:** Existing startup fallback behavior remains unchanged. Corresponding live reload cases reject the complete candidate rather than adopting startup defaults.

### E11 — Existing Font Selection Fallback

- **Given:** A valid non-empty configured font family name that is unavailable, with a usable system primary available; separately, a controlled inability to obtain any usable font resource.
- **When:** Manual reload prepares terminal fonts.
- **Then:** The former uses the established system-primary fallback with its warning and coherent effective metrics; the latter rejects reload and preserves the accepted state. Missing-glyph fallback does not change primary grid metrics or widget UI fonts.

## Decisions

| Material choice | Reason and authority |
| --- | --- |
| One manual application command with configurable default `Ctrl+Shift+R` | C1; issue #172 requires invocation without waiting for palette UI, and ADR 0040 supplies the existing dispatcher/precedence boundary |
| Strict live candidate acceptance, separate startup recovery | Issue #172, C1, ADR 0059; malformed live input must retain the last accepted generation rather than substitute defaults |
| Terminal font changes are live and terminal-only | C2, ADR 0059; this explicitly overrides only the issue's font/resource exclusion and preserves independent UI font ownership |
| Normal bounded resize, not conservative losslessness checks | C3, ADR 0060, spec 0014 and ADR 0052; preserve session continuity and newest bounded content rather than every historical cell |
| Preserve OSC RGB overlays and advance reset baseline | C1, ADR 0059, partially extending ADR 0036/0053; no new OSC syntax or cross-session inheritance |
| Keep existing font-selection/glyph fallback | Existing System Default Font Selection context and `font.rs` / `DwriteState::open_primary_with_sink`; this spec does not introduce a new strict family-availability rule |
| Shell changes are new-session-only; clipboard changes require restart | C1, ADR 0059; reporting application policy is not permission to restart existing children or silently change clipboard authorization |
| Non-modal visible results plus diagnostics | C1 and issue #172; a manual operation needs actionable success/failure feedback without shell-output injection or a palette dependency |

## Test Plan and Delivery Evidence

- **Focused automation:** Cover R2's complete validation and startup/live distinction, command registry/default/conflict/unbinding cases, accepted-state/factory coherence, palette override identity and alpha/reset semantics, font-independent UI metrics, bounded retention, projection/cache invalidation, and failure injection for E7. Exercise all-session acceptance, not merely individual loader/renderer return values.
- **Windows runtime:** Record real `cmd` and PowerShell font changes with prompt/pending-input continuity and subsequent typing/backspace/Enter; active/inactive tabs; a real alternate-screen application such as `nvim`; selection/copy of surviving text; the supported IME path; backdrop and opaque fallback appearance; and byte-level consumed-chord evidence on the actual supported PTY transport. Parser/model tests alone do not prove ConPTY, keyboard, or GPU compatibility.
- **Resource transitions:** Exercise repeated font/size changes, returning to an earlier configuration, font-selection fallback, hidden-tab rendering after acceptance, and failed preparation/retry. Verify effective font/metric/cache generation coherence rather than only comparing requested settings. Retain applicable existing terminal font replacement, resize barrier, and reflow regressions.
- **Quality gates at delivery:** `cargo fmt --check`; `cargo clippy --all-targets --all-features -- -D warnings`; `cargo test --workspace`; `python scripts/check_docs.py`; `python scripts/checklist_summary.py`. Record skipped or blocked commands and reasons rather than implying a pass.
- **Evidence format:** Record revision and dirty-tree scope, OS/Windows build, relevant ConPTY/application versions, exact claimed behavior, commands/reproducible steps, expected and observed results, artifacts, exclusions, and `PASS` / `FAIL` / `NOT RUN` / `BLOCKED`. Store self-contained durable summaries under `docs/verification/` or link retrievable CI artifacts. Do not publish secrets, unrelated terminal content, or links requiring local-only artifacts.
- **Documentation:** Reconcile user-facing startup-only descriptions and supported live policies when implementation exists. Reconcile the GitHub issue's font exclusion before claiming the expanded issue delivered. Spec creation, documentation checks, and an ADR's proposed status are not product/runtime acceptance evidence.

## Out of Scope

- Automatic file watching, debounce, editor-save burst handling, or a file-watcher lifecycle.
- Command-palette UI; registration/discovery metadata is required, not palette implementation.
- Widget-library HMR changes, application/day-night mode changes, or UI/chrome/dialog font configuration.
- Restarting/relaunching running PTYs, child processes, or the application to apply reload.
- New shell profile, environment, launch-directory, font-file override, or other settings schema not present in the current supported configuration.
- Permanent retention of every old history row or clipped alternate-screen cell, unlimited history, hidden off-grid backups, or a special lossless font-resize subsystem.
- New OSC color syntax, special palette indexes, changed reply limits, or broader protocol support.
- Live clipboard-policy changes; malformed candidates cannot be partially accepted under the restart-required classification.

## Completion Boundary

This document is a requirements contract for the selected spec-only endpoint. It does not create an implementation plan or ticket set, implement reload, update the remote issue, or claim any product quality gate/runtime scenario has passed. The issue's font-scope discrepancy is explicitly resolved for this local contract by C2/ADR 0059 and qualified by C3/ADR 0060; remote wording remains unchanged until separately authorized.

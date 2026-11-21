# Pane Workspace with Environment-Aware Splitting

**Spec ID:** 0056
**Status:** Settled requirements for new scope; not implementation or runtime acceptance
**Date:** 2026-11-16
**Sources:** [Issue #158](https://github.com/wangjq4214/harbor/issues/158), read with `gh issue view 158 --json number,title,body,comments`; and the owner refinement summarized below.
**Endpoint / permissions:** Spec and local ticket set only. No production changes, GitHub mutations, CONTEXT/ADR writes, staging or commits are authorized by this refinement. This new scope is not included in the historical acceptance baseline in the grimoire README.

## Source Handoff

The issue requires horizontal/vertical splits, draggable ratios, click/shortcut focus, close/collapse, maximize/restore, independent allocation/DPI/minimum-size/PTY resize, scheduling of all visible panes, bounded hidden output, correctly routed input and preserved paste confirmation. It explicitly separates workspace, pane and session ownership and excludes cross-window movement and persistence.

Settled owner decisions, in conversation order:

- **C1:** A split creates a new independent session and defaults to the source pane's current directory.
- **C2:** `Ctrl+W` closes the focused pane; `Ctrl+Shift+W` closes the entire active tab. Closing the last pane closes its tab; closing the final tab closes the window.
- **C3:** Hidden panes retain their most recent valid grid dimensions, continue bounded output processing, and resize to their real allocation when displayed again.
- **C4:** Natural shell-session exit automatically closes that pane and collapses its layout, with the same last-pane/tab behavior as an explicit close. This is the session shell exiting, not an arbitrary foreground program finishing.
- **C5, final correction:** On a split action, automatically enter the source's same execution environment and current directory, including WSL/SSH. If recognition, connection or directory entry fails, automatically fall back to the default shell configuration. This replaces both the earlier local-only fallback suggestion and the owner's intermediate no-fallback preference.
- **C6:** The owner selected spec followed by local tickets. CONTEXT/ADR recording was not selected.

## Requirements

### R1 — Workspace, pane and session ownership

Represent a window's ordered tabs as workspaces containing a split tree, whose leaf panes have explicit session bindings. A pane owns presentation allocation, visibility and focus; a session retains terminal state and PTY lifetime independently of whether its pane is mounted or visible. Use stable workspace/pane/session identities, or an equally explicit ownership model, rather than treating a reused pane position or the active tab as evidence of session identity.

A split creates an independent terminal/session, not a second view of the existing PTY. Source and new sessions must retain separate buffers, scrollback, selection, protocol state, output and shutdown. Bind each external draw registration deliberately to the correct live presentation/session. Closing and delayed callbacks cannot retarget a surviving or replacement session.

The Host/application owns session creation, launch policy, split meaning, workspace membership and shutdown. Generic layout, focus, pointer capture and external-surface mechanisms remain widget responsibilities; the terminal engine remains GPU-independent. This does not select a new crate, global bus, concrete arena type or universal renderer interface.

### R2 — Split into the source environment and current directory

A user can split the focused pane horizontally or vertically, producing a new independent interactive session in the same execution environment and current directory. The action must preserve the source session and attach the result only to the still-valid intended workspace/pane operation. Later focus changes, metadata changes, tab switches, source closure and stale completions must not silently substitute another source or attach into another workspace.

Use current environment-aware directory information, not the source session's initial launch directory, an earlier agent's directory, a raw path interpreted in the wrong environment, or whichever session is active at delayed completion. Local, WSL and SSH directories retain their execution-environment identity: WSL launches into the identified distribution; SSH connects to the identified target; neither path is reinterpreted as a local Windows path.

The existing project integration and environment-launch boundary are relevant reuse surfaces, not proof that split behavior exists. Baseline verification must exercise local pwsh, WSL bash and direct-SSH bash, the established project-integration cases. Unknown or unusable context follows R3 rather than pretending inheritance succeeded. Record the recognition coverage actually exercised; no zero-configuration inference or unrestricted nested-environment support is inferred from these baseline cases.

Current recognition is opt-in and partly user-declared: `scripts/shell/harbor-project.bash` is sourced with a WSL distribution or SSH target and reports `$PWD`; it does not discover or verify that target. `scripts/shell/harbor-project.ps1` reports local location. Reusing these observations does not establish that an arbitrary declared target reproduces the source; actual environment/cwd observations are required in E1–E2.

The user-requested split itself authorizes the required connection and directory entry. Merely receiving OSC metadata, rendering, hovering or validating configuration must not establish a connection, execute a directory change or trigger a split. Preserve argument/path boundaries and centralized launch validation/quoting. Reuse normal environment authentication; do not persist credentials, dump runtime environments, replay arbitrary command history, clone a foreground process, or install remote/local integration without authorization. This split behavior does not automatically relaunch the source's recognized agent; that separate policy belongs to saved project presets.

### R3 — Automatic fallback to the default shell configuration

If source-environment/current-directory recognition is unavailable or unusable, or the inherited connection or directory-entry attempt fails, create the new pane using the accepted default `[shell]` launch policy. This includes its normal executable/arguments, configured initial-directory semantics and child environment policy. Do not combine the default local shell with the failed source's remote directory or stale source-specific launch fields.

A successful launcher process creation alone is not proof of a successful SSH connection or directory entry. Acceptance must distinguish inherited-environment readiness from startup failure, including a directory that disappears between recognition and launch. Clean up a failed partial launch before publishing a successful replacement; do not leave duplicate panes, PTYs, readers or late results that can take over the fallback pane.

Inspected implementation limitation: `src/profile_launch/project.rs` constructs environment-specific commands, but its local `Set-Location`/`-NoExit` and WSL/SSH `cd`/interactive-shell templates provide no Host-observable, attempt-qualified startup-readiness result. `TerminalTabFactory::create_resources_with_command` returns after PTY/resource creation. Readiness/failure detection and its lifecycle are new split delivery work, not an existing guarantee supplied by those templates.

Default-shell fallback retains the existing ordinary launch pipeline and its diagnostics, executable recovery and failure semantics; it is not permission for recursive reconnect/relaunch attempts. If the default launch also fails, report failure and preserve surviving sessions without a falsely live pane or orphaned process. After a session has successfully started, its later natural exit follows R7, not automatic fallback/restart.

Fallback is specifically authorized for this split operation. It must not change named-profile fail-visible behavior, project-preset capture's explicit unknown/correctable behavior, ordinary New tab launch semantics or default configuration validation/reload. The feature must not claim that a default-shell result inherited the source environment/directory. Existing failure/notification presentation can report the degraded result without exposing credentials or terminal contents; no new confirmation step is selected.

### R4 — Split layout, allocation, drag and maximize/restore

A tab supports horizontal and vertical splits with several simultaneously visible panes, including mixed split directions through the split-tree model. Ratios can be dragged. Each leaf receives an independent allocation, logical-to-physical/DPI conversion, minimum-size handling and valid terminal grid/PTY resize target. Document the concrete minimum-size rules without inventing a numeric requirement here. Invalid, zero or duplicate targets must not be sent to the PTY.

Maximizing one pane preserves the split structure, ratios and other sessions; restore returns to the split layout using the current window allocation/DPI. Hidden panes follow C3/R5. Closing a leaf collapses redundant layout structure without recreating or accidentally closing survivors.

Preserve content/reflow behavior and the acknowledged resize barrier's exactly-once byte ordering, latest-target coalescing, stale-epoch rejection and failure/reader-release guarantees. Ordinary allocation-driven resize remains bounded across the host turn and fairly advances applicable sessions. Dragging must not turn every geometry notification, draw or output callback into an unbounded synchronous sweep.

Font/configuration reload must still coordinate all applicable live sessions and renderers under the existing coherent acceptance/compensation contract. It must not broadcast the focused pane's dimensions to differently sized siblings or lose session identity. Hidden-pane retained grid sizes do not exempt those sessions from applicable accepted font/color/binding updates. This spec changes ordinary allocation propagation, not the separate configuration transaction's acceptance guarantees.

### R5 — Visibility and bounded output/render scheduling

Distinguish focused, visible-but-unfocused and hidden panes. Every visible pane can present output and contribute the applicable external schedule demand; keyboard focus is not a prerequisite for drawing fresh output. Hidden panes, including panes concealed by maximize and panes in inactive tabs, continue bounded PTY ingestion without hidden terminal draw calls or hidden cursor-driven scheduling.

Hiding, maximizing a sibling, switching tabs and replacing a UI root must not terminate the session. Hidden panes retain their most recent valid grid until displayed; restoration submits the newly observed valid allocation without a global one-size broadcast. Existing output/backpressure and bounded resize progress must remain responsive with several producers, rather than starving hidden sessions or requiring new child output to finish ready resize work.

Do not introduce per-frame full-history copying/scanning. Search and decoration work must retain their bounded session/revision-qualified contracts. Preserve idle scheduling when visible content is quiet. Measurements must report actual work/resource behavior; no new numerical frame or latency promise is selected.

### R6 — Correct input, commands and confirmation lifetimes

Click and keyboard commands can focus a pane. Keyboard/IME input and focused-session operations target the intended focused pane; pointer hit testing and capture target the appropriate pane and local coordinates. Selection, copy/paste, mouse reporting, wheel and scrollbars must not use the full-window or another pane's coordinates. Dragging a divider cannot leak pointer events into terminal mouse reporting. Focus loss, unmount, close and lifecycle cancellation release capture and clear/retire transient IME state as required by the existing input contract.

Migrate the current single-terminal focus/allocation bridge and tab-qualified command consumers to explicit pane/session targets. Search, profile capture, title/cwd metadata, hyperlink activation and clipboard requests cannot substitute the tab or a renderer ID for the source session. This preserves their existing behavior rather than adding cross-pane search or new shell workflows.

Use the established typed command registry, source-aware dispatcher and configurable shortcut mechanism. Matched consumed application chords execute once without PTY bytes; overrides/unbinding/conflict validation retain their existing semantics. The exact new split/focus/maximize shortcut chords and new command-ID spellings are implementation details; do not silently change the meaning of the existing close-tab command ID into close-pane. Its default binding changes under R7, while explicit configured close-tab intent remains close-tab intent.

Preserve the existing paste-confirmation safety gate and its independent confirmation-window behavior, while adding explicit original-session lifetime protection for pending user paste. Currently `src/dialog.rs::complete_paste_confirmation` receives the then-current terminal rather than retaining the original session target; the gate prevents user-driven changes, but natural-exit auto-close adds another race. A pending paste or OSC 52 request must be bound to its original live session and cannot land in a newly focused or replacement pane. Apply existing cancellation/gating rules to pane/session focus and closure, not just tab changes; visibility alone must not grant an unfocused session foreground clipboard-write permission. Do not introduce a second clipboard authorization path or treat the new split connection permission as clipboard permission.

### R7 — Close shortcuts, natural exit and cleanup

With default bindings, `Ctrl+W` closes only the focused pane and `Ctrl+Shift+W` closes the entire active tab and all its sessions. Existing tab-rail close intent remains a whole-tab operation. Closing one pane selects an eligible surviving pane deterministically and preserves its process, terminal contents, split placement and input routing; the concrete successor selection algorithm is not fixed here.

Natural session-shell exit automatically removes that pane and collapses its layout. A foreground command finishing while the shell continues is not pane exit. Closing the final pane closes its tab; closing the final tab closes the main window without creating a replacement session.

Explicit close, natural exit, output, pending startup/fallback, pointer capture, confirmation, search and resize may race. Each session must be retired once; late output/exit/ACK/launch results are safely ignored or cleaned up rather than applied to another session. Release the intended PTY/process ownership, reader, terminal, bridge registrations/callbacks and pending work without orphan processes or ownership cycles. Hiding or switching tabs is never equivalent to close.

### R8 — Evidence, scope and truthful completion

Provide focused model/command tests, mounted input/layout tests, offscreen rendering where appropriate, real Windows/ConPTY tests and reproducible runtime observations. A parser test, mocked launcher, closed predecessor issue or historical owner acceptance does not establish pane interoperability or successful remote directory entry.

Record revision and dirty-tree scope, Windows/ConPTY and application/shell/WSL/SSH versions, integration setup and connection prerequisites, exact steps/commands, expected versus observed results, retrievable artifacts, limitations and PASS / FAIL / NOT RUN / BLOCKED. Use synthetic paths/values and controlled hosts; do not include secrets or unrelated terminal contents. Missing WSL/SSH credentials or hosts are BLOCKED/NOT RUN, not a passing fallback test or evidence that same-environment splitting worked.

Use current [testing guidance](../../docs/developer/testing.md). The historical issue names `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test --workspace`, `python scripts/check_docs.py` and `python scripts/checklist_summary.py`. At refinement time the docs checker is `scripts/docs/check_docs.py`, the checklist script and `docs/validation.md` are absent, and current resource suites use the documented `just`/Windows wrapper routes. Resolve current equivalents and record omitted/obsolete commands honestly rather than recreating old tooling or claiming it ran.

Keep #158 open until the full accepted scope has evidence or an explicit scope decision links the remainder. Status/protocol/user documentation changes during implementation must describe actual delivered behavior. Creating this spec/ticket set neither updates GitHub nor establishes execution readiness evidence.

## Solution and Integration Seams

| Seam | Connects | Required exchange / guarantee |
| --- | --- | --- |
| Identity / lifetime | Host session owner, tab workspace, split-tree pane, terminal and bridge | Stable explicit relationships, independent lifetime and stale-event rejection; no slot/active-tab alias |
| Source recognition | Session-scoped project context / directory observation → split launch policy | Current environment-qualified directory and valid source/operation identity, or explicit unusable state |
| Launch / fallback | Split command → Host/factory → existing environment launcher and default shell pipeline | User-triggered connection/entry, distinguish startup readiness/failure, one resulting session or honest failure, failed-attempt cleanup |
| Layout / resize | Split layout and per-pane allocation → Host/terminal transaction | Per-session valid targets, correct DPI/local geometry, retained hidden grids and bounded ordered progress |
| Presentation / scheduling | Pane visibility → bridges / external surfaces → widget scheduler | Register/schedule all visible panes, no hidden draw/blink loop, no destruction on unmount |
| Input / command | Widget focus/hit testing/capture and command dispatcher → pane/session | Correct target/coordinates, consumed/pass-through semantics, correct IME and focused-session operations |
| Async permissions / results | Paste/OSC 52/search/configuration transactions → originating sessions | Identity/lifetime/revision checks, unchanged safety and coherent acceptance, no active-pane substitution |
| Exit / shutdown | Session reader/process completion and explicit close → workspace reduction | Idempotent retirement, layout collapse, survivor focus, final-tab/window behavior and complete resource release |

Inspected integration surfaces include `crates/harbor-app/src/tab_manager.rs`, `workspace.rs`, `command.rs`, `terminal_view.rs`, Host `src/tab_coordinator.rs`, profile launch/capture and shell integration. These identify responsibilities, not mandated new filenames or incidental Rust signatures. Reuse existing recognition/launch infrastructure after checking its current capabilities; do not treat proposed ADR text as executed evidence.

## End-to-End Acceptance

| ID | Given / action | Required observation |
| --- | --- | --- |
| E1 | In local pwsh, change directory away from initial launch cwd, then split horizontally and vertically | Independent children report the source's split-time current directory; original shell and content remain intact |
| E2 | In WSL bash and direct-SSH bash with current context recognition, change cwd and split | New interactive shell reports the same distribution/target and requested cwd; no Windows reinterpretation and no copied foreground process |
| E3 | Missing/invalid context, failed SSH connection, unavailable WSL target, or directory removed before entry | Automatic default `[shell]` result, correct default cwd/args/environment, no stale remote path, partial child, duplicate pane or false inheritance claim |
| E4 | Make the default launch fail as well; delay an inherited-launch failure/result while closing its source/workspace | Honest failure; survivors remain usable; no orphan process or attachment into a different/new workspace |
| E5 | Create several mixed-direction panes, run concurrent output, drag ratios and resize across DPI changes | Independent correct grids, clipping and mouse coordinates; bounded/fair output and resize progress; no zero/duplicate invalid target or output corruption |
| E6 | Maximize one pane, continue output in hidden siblings, resize/change DPI and restore; repeat in inactive tabs | Sessions survive, hidden grids retain last valid dimensions until shown, no hidden draw/blink demand, restored allocations converge correctly |
| E7 | Switch focus by click and shortcut under simultaneous output; type distinct markers, use IME, selection/copy/paste, wheel, mouse reporting and capture | Input goes only to its intended session with pane-local coordinates; divider drag does not generate terminal input; no leaked UI chord or stale composition |
| E8 | Start paste/OSC 52 confirmation and search, then exercise allowed close/focus/exit races and delayed completion | Existing gate/foreground policy holds; no action or result reaches another session; retired targets cannot be revived |
| E9 | Use default close shortcuts, rail close and shell `exit`; finish a foreground program without exiting the shell | Pane-only versus whole-tab behavior is distinct, foreground completion alone preserves the pane, close/exit collapses only the intended subtree; final close exits the window |
| E10 | Override/unbind close and pane commands, introduce a binding conflict, invoke from applicable UI/command sources | Existing atomic binding validation/dispatch survives; explicit close-tab configuration still closes a tab; consumed input never reaches a PTY |
| E11 | Reload fonts/colors/bindings with unequal visible/hidden pane sizes, including rejection/compensation failure | Coherent accepted generation, preserved session identity, correct per-pane geometry and existing recovery behavior; no PTY restart or one-size broadcast |
| E12 | Repeated split/close/exit/restore and launch-fallback cycles with pending output/resize; observe quiet and sustained-output states | All visible panes update, hidden sessions progress within bounds, no leaks/starvation/hidden polling, survivors responsive; exact measurements and limitations recorded |

## Decisions, Compatibility and Recording Boundary

- **Issue #158 + C1–C5:** define the workspace outcome and added same-environment/current-directory split/fallback behavior. This spec is the selected requirements artifact; no CONTEXT/ADR edits were authorized.
- **[Spec 0010](./0010-desktop-terminal-tabs-and-view-macro.md) / [spec 0013](./0013-configurable-application-keybindings.md):** this new scope deliberately replaces the historical one-tab/one-session, active-only terminal painting, common allocation broadcast and default `Ctrl+W` close-tab assumptions. Preserve independent-session, final-window, generic widget and command-dispatch guarantees. Ordinary tab navigation remains tab navigation.
- **[ADR 0033](../adr/0033-osc7-metadata-without-filesystem-side-effects.md):** parser metadata remains inert. C5 authorizes environment access only as an effect of the user-requested split, not arbitrary remote OSC 7/local path reinterpretation.
- **[Spec 0052](./0052-named-session-profiles-and-quick-save.md), [ADR 0106](../adr/0106-independent-session-profiles-and-explicit-osc7-launch-policy.md), [ADR 0109](../adr/0109-environment-aware-project-presets-and-recognized-agents.md), [ADR 0110](../adr/0110-last-recognized-agent-launch-with-original-arguments.md):** reuse recognized context and launch seams, retaining credential and opt-in integration boundaries. C5's automatic fallback applies only to split; named-launch failure, save-time correction and agent-relaunch policy are not globally rewritten.
- **[ADR 0095](../adr/0095-event-driven-resize-barrier-and-shared-reader-dispatch.md) / [ADR 0096](../adr/0096-bounded-multi-tab-resize-scheduling.md):** preserve acknowledged ordering, shared dispatch, bounded/fair progress and coherent reload; C3/R4 replace common ordinary-allocation broadcast with per-pane visible targets/hidden retained grids. This does not authorize moving all expensive operations off-thread or promise preemption of one expensive operation.
- **[ADR 0064](../adr/0064-widget-and-application-component-ownership.md), [ADR 0040](../adr/0040-application-command-dispatch-and-keybinding-precedence.md), [ADR 0049](../adr/0049-osc52-confirmation-window-and-foreground-policy.md), [spec 0027](./0027-revisioned-terminal-read-model-and-literal-search.md):** preserve responsibility, command, clipboard and result-provenance contracts while migrating to explicit pane/session targets.

**Unpersisted knowledge candidates:** workspace/pane/session terminology and the split-triggered environment-access policy, including its narrow default-shell fallback exception. Recording permission is absent, so they remain here as requirements/traceability rather than edits to CONTEXT or ADR history. This does not block the selected spec/ticket endpoint.

**Implementation details left open, not requirement gaps:** concrete ID/storage types, split component API, numeric minimum geometry and per-turn budgets, exact additional shortcut chords, focus-successor algorithm, startup-readiness signaling and connection/cwd-failure detection mechanism. They must satisfy the observable contracts above; no new domain policy may be inferred from leaving these details open.

## Out of Scope

Cross-window movement, persistence/restoration, dynamic plugin/renderer hosting, production Kitty graphics, shared-session mirrored views, arbitrary process/command-history cloning, automatic agent relaunch on split, credential persistence, and unsolicited integration installation. This feature does not add global cross-session search, replace terminal text/reflow storage, or change unrelated preset-save and explicit named-launch contracts.

# Pane Workspace — Issue #158

**Source:** [Spec 0056](../../spec/0056-pane-workspace.md), including its C1–C6 owner handoff, R1–R8 requirements and E1–E12 acceptance; [GitHub #158](https://github.com/wangjq4214/harbor/issues/158).
**Ticket folder:** `.grimoire/ticket/0002-pane-workspace/`
**Status:** Todo; requirements/decomposition only, no implementation or runtime acceptance.

## Overview

Deliver independently routed terminal panes in a tab workspace: horizontal/vertical splitting, draggable ratios, maximize/restore, close/collapse, per-pane DPI/PTY geometry, visible-pane scheduling and bounded hidden output. A split enters the source's current environment/directory, including WSL/SSH; recognition/connection/directory-entry failure automatically falls back to the accepted default `[shell]` policy. Default `Ctrl+W` closes the focused pane, `Ctrl+Shift+W` the entire tab; natural shell exit closes the pane. Hidden panes retain their last valid grids until displayed.

Do not confuse same-environment splitting with process cloning or saved-agent relaunch. Preserve paste/OSC 52 permissions, session/revision-qualified results, typed commands, coherent configuration acceptance and ordered/bounded resize. No cross-window movement or persistence.

## Permissions and Knowledge Boundary

The selected route is refine → spec → slice. These local artifacts do not authorize implementation, QA execution, CONTEXT/ADR edits, GitHub mutation, staging or commits. Implementation may begin only under a separate execution request.

Consume the settled spec and its cited sources. Organize, decompose and sequence them without inventing or changing domain facts, architecture, requirements, constraints or acceptance semantics. If a new material decision is needed, return it to refinement rather than invent it downstream. Existing requirements and compatibility changes are stated in spec 0056; ordinary named-profile and preset-save behavior remain separate.

Unpersisted knowledge candidates remain the workspace/pane/session terminology and the split-specific user-triggered environment-access/fallback policy. Recording permission is absent; this is not a blocker for the selected local artifacts. Do not edit earlier ADRs/specs to disguise their scoped differences.

## Delivery Surfaces

- Application model: `crates/harbor-app/src/tab_manager.rs`, workspace/tab presentation, command registry and session-qualified async consumers.
- Host/factory: `src/tab_coordinator.rs`, launch/capture and environment-launch boundaries, accepted default shell policy.
- Existing opt-in environment metadata: `scripts/shell/` and terminal project-context storage; parsing must remain inert.
- Generic widgets: layout, allocation observation, focus, pointer capture, external-surface registration and scheduler integration, without Harbor-specific business policy.
- Terminal/PTY/renderer: per-session grid transactions, output/exit dispatch, rendering projections, input, shutdown and resource release.
- Configuration, clipboard/paste, search/decorations and documentation/evidence must migrate without weakening their current contracts.

Paths describe inspected integration responsibilities, not a prescribed file/module redesign.

## Dependencies

| Producer | Consumer | Why consumer cannot complete first |
| --- | --- | --- |
| T0001 | T0002 | Split launch must capture and revalidate explicit source/session/operation identity rather than active-tab aliases. |
| T0001 | T0003 | Several pane allocations, visibility states and bridge registrations require separate workspace/pane/session ownership. |
| T0002 | T0004 | Production split commands need the required environment/current-directory launch and automatic fallback lifecycle. |
| T0003 | T0004 | Real multi-pane routing, capture, close/collapse and maximize/focus interaction require mounted pane geometry and visibility. |
| T0004 | T0005 | Full runtime acceptance requires the integrated launch, layout, input and lifecycle behavior, not isolated fixtures. |

The graph is acyclic: T0001 → {T0002, T0003} → T0004 → T0005. Component tests and evidence preparation may start earlier; these edges govern complete integration/acceptance, not every line of preliminary work.

T0001 is a separately verifiable identity migration with several existing consumers: PTY events, search, clipboard, configuration and draw registrations. T0002 has an independently testable process/connection/startup/fallback lifecycle. T0003 has independently testable mounted geometry/scheduling behavior. These are not cosmetic global refactorings. None alone is a claim of user-ready split support; production split command admission belongs to T0004.

## Coordination

| Tickets | Risk | Strategy |
| --- | --- | --- |
| T0001–T0004 | Session identity is accidentally replaced by pane/tab/draw identity | Share the explicit binding and stale-operation contract from R1–R3/R6–R7; preserve existing read-model identities rather than inventing a competing revision scheme. |
| T0002, T0003 | Launch resource binding and layout attachment share Host/model surfaces | Agree on identity-qualified creation/completion and allocation exchange after T0001; sequence overlapping edits, not a fabricated semantic dependency. |
| T0003, T0004 | Visibility versus focus and capture versus hit testing become conflated | T0003 owns geometry/visibility observations; T0004 validates production focus/input/lifecycle against those observations. |
| T0001, T0003, T0004 | Configuration reload, search and clipboard still assume one session per tab | T0001 migrates identity; T0003 preserves distinct geometry/coherent reload; T0004 verifies focused-source safety and close races. |
| T0002, T0005 | A spawned ssh/wsl launcher is reported as successful inheritance | Verify actual connection/environment/cwd and failed-startup cleanup; record missing real hosts as BLOCKED/NOT RUN, not a pass. |

## Recommended Order

1. T0001: migrate ownership/identity with single-pane regression evidence.
2. T0002 and T0003 may proceed in parallel with the coordination above: inherited launch/fallback and mounted layout/scheduling.
3. T0004: integrate the user-facing split/focus/close commands and multi-pane input/confirmation/exit behavior.
4. T0005: execute the full Windows/runtime/resource matrix and reconcile documentation to the delivered scope.

Do not expose partially integrated split commands or advertise success from an intermediate component ticket.

## Requirement Coverage

| Spec scope | Responsible tickets |
| --- | --- |
| R1 ownership, identity, lifetime | T0001; integration/close races in T0004 |
| R2 source environment/current directory | T0002; command attachment in T0004 |
| R3 startup failure/default-shell fallback | T0002; integrated races in T0004 |
| R4 splits/ratios/DPI/minimum sizes/resize/maximize/reload | T0003; command/close integration in T0004 |
| R5 visible/hidden output, scheduling, retained hidden grids | T0003 |
| R6 focus/input/commands/permissions/async results | T0001 identity migration; T0004 multi-pane behavior |
| R7 shortcuts/natural exit/collapse/final window/cleanup | T0004 |
| R8 evidence and truthful scope | Focused acceptance in every ticket; T0005 integrated matrix |
| E1–E4 | T0002 + T0004; real-environment acceptance T0005 |
| E5–E6 | T0003 + T0004; runtime acceptance T0005 |
| E7–E10 | T0004; runtime acceptance T0005 |
| E11 | T0001 + T0003 + T0004; integrated acceptance T0005 |
| E12 | T0003 + T0004; resource/runtime measurements T0005 |

## Ticket Index

| Ticket | File | Outcome |
| --- | --- | --- |
| T0001 | [T0001-workspace-pane-session-identity.md](./T0001-workspace-pane-session-identity.md) | Explicit independent ownership and source-qualified routing, preserving single-pane operation. |
| T0002 | [T0002-environment-aware-split-launch.md](./T0002-environment-aware-split-launch.md) | Same-environment/current-directory launch with automatic default-shell fallback and failed-attempt cleanup. |
| T0003 | [T0003-pane-layout-resize-and-scheduling.md](./T0003-pane-layout-resize-and-scheduling.md) | Mixed split layout, draggable ratios, maximize/restore, per-pane geometry and correct visible/hidden scheduling. |
| T0004 | [T0004-pane-input-commands-and-lifecycle.md](./T0004-pane-input-commands-and-lifecycle.md) | User-facing splitting/focus, safe input/confirmation and pane/tab/exit lifecycle. |
| T0005 | [T0005-windows-acceptance-and-scope-evidence.md](./T0005-windows-acceptance-and-scope-evidence.md) | Reproducible integrated Windows/environment/resource evidence and truthful documentation. |

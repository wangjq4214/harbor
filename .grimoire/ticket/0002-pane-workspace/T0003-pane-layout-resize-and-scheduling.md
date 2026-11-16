# Lay Out, Resize and Schedule Independent Panes

**Ticket ID:** T0003
**Source:** [Spec 0056](../../spec/0056-pane-workspace.md), R4–R5, presentation portions of R1/R6, E5–E6/E11–E12.
**Status:** Todo

## Goal

Render several independent panes in a mixed-direction split tree with draggable ratios and maximize/restore, correct per-pane grids and visible/hidden scheduling. This is independently verifiable with mounted fixtures before user-facing split commands are admitted.

## Affected Surfaces

- Application workspace/split presentation and allocation observations.
- Generic layout/divider/capture mechanisms where required, without Harbor business types in widgets.
- Terminal bridges/external surfaces, renderer projection, Host resize admission, output wake/scheduling and configuration geometry.

## Approach

Replace one active bridge/allocation and common tab-size broadcast with explicit visible pane geometry and retained hidden grids. Keep visibility distinct from focus. Each visible pane has its correct external registration and schedule demand; hidden sessions remain owned and continue bounded output processing.

Preserve ordered asynchronous resize transactions, shared event dispatch, latest-target merging, fair bounded host-turn progress and coherent font/configuration acceptance. Do not implement a separate resize engine or promise that one expensive operation is preemptible.

## Dependencies and Coordination

- **Blocked by:** T0001, because layout leaves and external registrations need stable session/pane bindings.
- **Blocks:** T0004's real focus/capture/close integration.
- **Parallel with:** T0002 after T0001; fixture creation can validate layout independently of remote launch readiness.
- **Coordination risks:** T0004 must consume the same geometry/visibility observations used for rendering, not a second full-window coordinate model.

## Acceptance

- [ ] Horizontal, vertical and mixed-direction several-pane layouts keep each leaf bound to its intended independent session.
- [ ] Ratio dragging, current-allocation maximize/restore and close/collapse projections preserve session identity and stored split structure/ratios where applicable.
- [ ] Pane-local allocation/DPI conversion and documented minimum-size rules produce independent valid grids; zero/invalid/unchanged targets do not generate invalid or duplicate PTY requests.
- [ ] All visible panes can update/present without keyboard focus; hidden panes generate no terminal draws or cursor-driven scheduling.
- [ ] Panes hidden by maximize or inactive tabs retain their latest valid grid until shown, continue bounded output and converge to their actual allocation on restoration.
- [ ] Output, idle, draw and allocation paths share bounded resize admission; latest targets and background progress are retained without requiring a new byte or activation to finish ready work.
- [ ] Existing acknowledged byte ordering, stale-epoch rejection, reader release and resize recovery remain intact.
- [ ] Font/color/binding reload enumerates applicable live sessions and preserves coherent acceptance/compensation with unequal pane sizes, including hidden sessions; it neither restarts PTYs nor broadcasts one focused pane's grid.
- [ ] Mounted layout, scheduling and appropriate offscreen tests cover simultaneous output, clipping, DPI changes, hidden grids, minimize/restore, continuous drag and configuration rejection. Record actual work bounds/limitations without invented numeric performance claims.

## Out of Scope

Host environment recognition/fallback, clipboard authorization redesign and production close/split shortcut admission. This ticket supplies correct geometry/visibility mechanisms, not standalone permission to expose an unsafe partially integrated split feature.

# Terminal Frame Scheduling and Standalone Host

**Spec ID:** 0006
**Status:** In Progress
**Date:** 2026-09-11

## Requirement

Terminal cursor blinking must continue while idle through deadline-driven redraws in both widget-hosted and direct winit/wgpu rendering modes, without Terminal owning window, surface, submission, or presentation resources.

## Solution

Expose a host-neutral Terminal Frame Demand that reports whether a frame is immediately needed and the earliest next cursor-blink deadline. Terminal input and cursor movement reset the blink cycle to its visible phase and update that demand.

For widget hosting, extend the `CustomPaint` external-provider contract with a scheduling callback. The Runtime collects frame demands before choosing its idle policy, merges the earliest deadline into its own effects, and lets the feature-gated winit integration and `Runtime Frame Scheduler` retain sole ownership of redraw requests, `WaitUntil`, surface acquisition, submission, and presentation. A non-drawable surface suppresses timer wakes; becoming drawable causes one recovery frame and recomputes future demand.

For direct rendering, provide a public feature-gated standalone winit/wgpu host adapter outside the Terminal core. It consumes the same demand, translates deadline expiry into winit control flow and redraw requests, and invokes Terminal only with a host-owned render pass. This preserves the core `harbor-terminal` boundary while allowing consumers to use Terminal without `harbor-widget`.

### Seams

| Seam | Connects | Expects | Provides |
| --- | --- | --- | --- |
| External draw scheduling | Terminal Widget Bridge ↔ harbor-widget Runtime | A pullable Terminal Frame Demand for each registered external draw | A Runtime-owned earliest deadline and redraw effect before idle wait selection |
| Direct terminal host | Standalone winit/wgpu adapter ↔ Terminal | Frame demand plus a host-owned wgpu render pass | Direct event-loop scheduling and terminal presentation without a Widget Runtime |

## End-to-End Tests

### E2E: Idle cursor blinks inside a widget frame

- **Given:** A drawable widget-hosted terminal with a visible blinking cursor and no pending PTY output or user input.
- **When:** The event loop reaches successive blink deadlines.
- **Then:** The Runtime requests one frame at each phase edge, the cursor alternates visible and hidden, and the event loop waits between deadlines rather than polling.

### E2E: Terminal input resets widget-hosted blinking

- **Given:** A widget-hosted cursor is in its hidden blink phase.
- **When:** Routed keyboard input or a cursor-position-changing terminal update occurs.
- **Then:** The next frame renders the cursor visible immediately and the next deadline starts a fresh blink cycle.

### E2E: Idle cursor blinks in the standalone host

- **Given:** The public standalone winit/wgpu terminal host is running with a drawable surface and an otherwise idle blinking cursor.
- **When:** The blink deadline expires.
- **Then:** The host requests and presents a frame through its own winit/wgpu path, producing the same visibility transition as widget hosting.

### E2E: Non-drawable surface suspends and recovers scheduling

- **Given:** Either host has a blinking terminal and its surface becomes zero-sized, minimized, or otherwise non-drawable.
- **When:** Blink deadlines pass and the surface later becomes drawable again.
- **Then:** No timer-driven frames are requested while non-drawable; restoration requests one frame and resumes deadline scheduling from the current terminal state.

## Decisions

### Use one host-neutral frame-demand contract

- **Choice:** Terminal reports immediate invalidation and the next blink deadline; it does not schedule a window or present a surface itself.
- **Reason:** A shared contract fixes idle blinking once and keeps widget and direct hosts behaviorally equivalent while preserving Terminal's injected-resource boundary.
- **ADR reference:** [0020-host-neutral-terminal-frame-scheduling](../adr/0020-host-neutral-terminal-frame-scheduling.md); [0011-terminal-custompaint-gpu-injection](../adr/0011-terminal-custompaint-gpu-injection.md)

### Make Runtime the widget-hosted scheduling authority

- **Choice:** Registered external draws contribute scheduling through Runtime-managed callbacks, and Runtime effects remain the only widget-hosted redraw and wait-policy output.
- **Reason:** Passing deadlines through `src/` would reintroduce App orchestration that ADR 0015 moved into the feature-gated widget integration.
- **ADR reference:** [0021-external-draw-scheduling-and-standalone-terminal-host](../adr/0021-external-draw-scheduling-and-standalone-terminal-host.md); [0015-runtime-owned-frame-presentation](../adr/0015-runtime-owned-frame-presentation.md)

### Keep direct winit support outside the Terminal core

- **Choice:** The direct-mode API is a feature-gated companion host adapter, not a `winit` dependency of the core `harbor-terminal` engine.
- **Reason:** ADR 0021 requires direct winit/wgpu support, while ADR 0012 explicitly keeps window management and `winit` out of `harbor-terminal`; an adapter boundary satisfies both decisions.
- **ADR reference:** [0021-external-draw-scheduling-and-standalone-terminal-host](../adr/0021-external-draw-scheduling-and-standalone-terminal-host.md); [0012-consolidate-render-ui-into-terminal](../adr/0012-consolidate-render-ui-into-terminal.md)

### Suspend timer wakes for non-drawable surfaces

- **Choice:** Hosts suppress blink-triggered redraws while their surface cannot draw and request one recovery frame when it can.
- **Reason:** This preserves the surface suspension and recovery policy already assigned to the winit integration while preventing background timer work.
- **ADR reference:** [0021-external-draw-scheduling-and-standalone-terminal-host](../adr/0021-external-draw-scheduling-and-standalone-terminal-host.md); [0015-runtime-owned-frame-presentation](../adr/0015-runtime-owned-frame-presentation.md)

## Test Plan

- **Integration tests:** Cover callback registration and deadline merging in `harbor-widget`; cover Terminal Frame Demand transitions for hidden/visible phases and reset events; cover the direct adapter's `WaitUntil`/redraw behavior without requiring a real GPU surface where possible.
- **Manual tests:** Run the application in widget mode and the standalone sample/host, leave each idle for several cycles, type while the cursor is hidden, minimize and restore the window, and verify the cursor resumes correctly.
- **Performance thresholds:** With blinking as the only activity, each host must wait between phase edges and request no more than one redraw per elapsed blink edge; it must not enter continuous `Poll` solely to blink the cursor.
- **Edge cases:** Steady cursor styles, terminal-hidden cursor, multiple external draw providers, delayed event-loop wakeups that cross multiple phases, zero-sized surfaces, surface loss/reconfiguration, and input arriving at a deadline boundary.

## Out of Scope

- Changing cursor style, blink interval configuration, or terminal protocol semantics.
- Moving window, surface, device, queue, or fatal-error ownership from the Runtime Host.
- Replacing terminal input routing, PTY invalidation, damage tracking, or paste-confirmation behavior.
- Generalizing external scheduling callbacks to unrelated animation providers beyond the contract required by Terminal.
- Adding non-winit standalone platform adapters.

## Future Evolution

- Generalize the proven external draw scheduling contract to other deadline-driven custom renderers when a second concrete provider exists.
- Add direct host adapters for additional platform event loops without changing Terminal Frame Demand.
- Revisit blink configuration and focus-state behavior if user settings or accessibility requirements require them.
- Re-evaluate scheduling granularity if terminal decorations or other animations add independent deadlines.

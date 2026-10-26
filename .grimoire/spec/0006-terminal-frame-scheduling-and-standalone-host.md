# Terminal Frame Scheduling and Standalone Host

**Spec ID:** 0006
**Status:** Accepted (owner-confirmed)
**Superseded in part:** the public standalone winit/wgpu host requirement (Solution, its decision, and direct-mode acceptance) is superseded by a newer decision not recorded in this repository; the retired ticket 0006/T0003 was marked Superseded.
**Date:** 2026-09-11

## Requirement

Terminal cursor blinking must continue while idle through deadline-driven redraws in both widget-hosted and direct winit/wgpu rendering modes, without Terminal owning window, surface, submission, or presentation resources.

## Solution

Expose a host-neutral Terminal Frame Demand that reports whether a frame is immediately needed and the earliest next cursor-blink deadline. Terminal input and cursor movement reset the blink cycle to its visible phase and update that demand.

For widget hosting, extend the `CustomPaint` external-provider contract with a scheduling callback. The Runtime collects frame demands before choosing its idle policy, merges the earliest deadline into its own effects, and lets the feature-gated winit integration and `Runtime Frame Scheduler` retain sole ownership of redraw requests, `WaitUntil`, surface acquisition, submission, and presentation. A non-drawable surface suppresses timer wakes; becoming drawable causes one recovery frame and recomputes future demand.

For direct rendering, provide a public feature-gated standalone winit/wgpu host adapter outside the Terminal core. It consumes the same demand, translates deadline expiry into winit control flow and redraw requests, and invokes Terminal only with a host-owned render pass. This preserves the core `harbor-terminal` boundary while allowing consumers to use Terminal without `harbor-widget`.

## Decisions

### Use one host-neutral frame-demand contract

See [ADR 0020](../adr/0020-host-neutral-terminal-frame-scheduling.md), [ADR 0011](../adr/0011-terminal-custompaint-gpu-injection.md).

### Make Runtime the widget-hosted scheduling authority

See [ADR 0021](../adr/0021-external-draw-scheduling-and-standalone-terminal-host.md), [ADR 0015](../adr/0015-runtime-owned-frame-presentation.md).

### Keep direct winit support outside the Terminal core

Historical direct-mode contract: a feature-gated companion host satisfies ADR 0021 without adding winit to the core. The partial-supersession note above remains unresolved. See [ADR 0021](../adr/0021-external-draw-scheduling-and-standalone-terminal-host.md), [ADR 0012](../adr/0012-consolidate-render-ui-into-terminal.md).

### Suspend timer wakes for non-drawable surfaces

See [ADR 0021](../adr/0021-external-draw-scheduling-and-standalone-terminal-host.md), [ADR 0015](../adr/0015-runtime-owned-frame-presentation.md).

## Test Plan

A visible idle blinking cursor must advance once per phase edge without polling. Keyboard input or cursor-position-changing output resets a hidden cursor immediately to visible. Drawable restoration requests one recovery frame and recomputes demand; apply the same contract to direct mode subject to the unresolved supersession note.

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

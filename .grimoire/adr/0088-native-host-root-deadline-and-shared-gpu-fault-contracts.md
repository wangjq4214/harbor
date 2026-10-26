# Native Host Roots, Non-Rendering Deadlines, and Shared GPU Faults

**Status:** Proposed
**Date:** 2026-10-26
**Sources:** [UI_API_REVIEW.md](../../UI_API_REVIEW.md) U5/F9/F13/F15; the 2026-10-26 refinement conversation approving the recommendations below and the spec → tickets endpoint.
**Related:** [ADR 0031](./0031-widget-winit-adapter-owns-native-host-infrastructure.md), [ADR 0079](./0079-defer-root-construction-and-unify-content-slot-boundaries.md).

## Context

The native Host still restricts root-factory results to Component although Runtime already accepts IntoChildView. Its non-drawable scheduler branch ignores host_deadline. SharedGpu creates a Device without connecting device-lost and uncaptured-error notifications to Host outcomes; the existence of FrameError categories and injected policy tests does not establish production error delivery.

ADR 0031 keeps reusable native infrastructure in harbor-widget::winit while the application owns EventLoop, multi-window coordination and fatal/exit policy. U5 completes those contracts rather than moving those responsibilities. A shared Device failure cannot be consumed as if it belonged only to the first window that sees it. In particular, the current OSC52 confirmation failure path only closes that window; this is not sufficient handling of a shared-device fatal condition.

The review allowed either retaining non-rendering host deadlines or restricting the parameter to rendering timing. The user selected the former. Automatic device reconstruction is not selected for this unit.

## Decision

1. **Use the established root-description protocol.** Both build and build_with_output accept IntoChildView + 'static results, including Components, macro Views and existing Views. Preserve one-shot factories, factory errors, bootstrap output, mounting timing and root/HMR identity contracts; do not make View implement Component merely to satisfy the old bound.
2. **host_deadline is independent of drawability.** A future Host deadline remains a WaitUntil demand when a window is zero-sized/non-drawable. Suspend drawing, render-animation polling and surface-recovery redraw work in that state. The application handles due business work and refreshes/removes its deadline; the Host does not execute business callbacks or repeatedly return a past WaitUntil. Restore normal rendering scheduling when drawable again.
3. **SharedGpu owns persistent device-level fault state.** Connect device-lost and uncaptured-error notifications to that state and a UI-thread wake path. Observation by one Host must not erase a fatal condition for other Hosts sharing the Device. Notification delivery must not depend on another redraw, a visible window or ongoing animation. GPU callbacks record and wake; they do not manipulate Runtime/Window or apply application exit policy on callback threads.
4. **Report fatal faults through explicit Host boundaries, including non-frame turns.** Frame encoding, submission and requested presentation must participate in the reporting path, not just surface acquisition. The UI thread can observe a fatal condition without forcing a drawable frame. Preserve asynchronous reporting: submitted work or requested presentation is not evidence of completed GPU execution, and no synchronous capture of every GPU error is promised.
5. **Use a bounded fatal policy, not automatic device recovery.** Out-of-memory, unexpected DeviceLost and uncaptured Validation/Internal errors are fatal. Expected normal device destruction must not be misreported as failure. Existing recoverable surface loss/outdated/suboptimal behavior remains distinct from shared-device fatal state. The concrete diagnostic/error and wake API shapes are implementation choices, not frozen by this ADR.
6. **Harbor exits on a shared-device fatal report.** The application diagnoses the failure and chooses exit; the widget crate reports it but never exits the process. Main, paste-confirmation and OSC52 confirmation paths cannot downgrade that report to dismissal of one dialog. Preserve cancellation and input/clipboard safety while handling the failure.
7. **Keep lifetime ownership and evidence boundaries.** Wake/error registrations must follow native Host/shared-GPU lifetimes and must not retain unloaded HMR-generation callbacks. Preserve native resource destruction order. Verify real production notification wiring, idle/minimized delivery and multi-window observation; direct FrameError injection is policy evidence only. Real device-failure verification remains separate from simulated errors and must be reported honestly when unavailable.

## Consequences

- Component/View root construction becomes consistent without a second root-description model or an HMR redesign.
- A minimized window may wake for application work without acquiring a surface, submitting a frame or polling animation.
- Error delivery needs a thread-safe shared state and a wake mechanism that remains effective while rendering is suspended. Registering callbacks alone does not prove that backend notifications are progressed and delivered.
- All affected application consumers must accommodate fatal reports outside ordinary redraw handling. A shared fatal state cannot be a destructive per-window inbox.
- U5 does not reconstruct Device/Queue, replay terminal resources, take EventLoop ownership, or change U6 renderer ordering. Existing U1–U4 and HMR teardown obligations remain.
- This record captures the approved direction, not implementation or acceptance. [Spec 0042](../spec/0042-native-host-roots-deadlines-and-gpu-outcomes.md) supplies the behavior and verification contract.

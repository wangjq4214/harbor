# Widget-Hosted Cursor Blink

**Ticket ID:** T0002
**Source:** [Spec: 0006-terminal-frame-scheduling-and-standalone-host](../../spec/0006-terminal-frame-scheduling-and-standalone-host.md)
**Status:** Todo

## Goal

A Terminal embedded through `TerminalWidgetBridge` visibly blinks while idle because the widget Runtime schedules its deadline-driven frames, and resumes correctly after input or surface restoration.

## Layers

- [ ] **harbor-terminal state and rendering:** Consume T0001's Frame Demand from the bridge and render the resulting cursor phase only into the Runtime-provided render pass.
- [ ] **harbor-widget Runtime / CustomPaint:** Extend external draw registration with a scheduling callback, collect its demand before idle, merge the earliest deadline into Runtime scheduling, and register the terminal bridge callback beside its draw provider.
- [ ] **Runtime Host / winit-wgpu adapter:** Apply the merged Runtime effects through `FrameScheduler` and the feature-gated winit integration; suppress deadline wakes while non-drawable and request a recovery frame when drawable again.
- [ ] **Verification:** Add widget/runtime and winit contract tests plus an end-to-end bridge scenario for idle blink, reset after routed input, and surface suspension/recovery.

## Approach

1. Add an external scheduling-provider contract adjacent to `ExternalDrawFn`, carried through `CustomPaint`, `BuildCx`, and Runtime registration using the same stable `ExternalDrawId` model.
2. During Runtime's pre-idle work, query registered scheduling providers and fold their immediate invalidation and earliest deadline into Runtime effects without allowing them to acquire, submit, or present a frame.
3. Extend `TerminalWidgetBridge` to register a scheduling provider that reads the terminal's Frame Demand independently of a draw invocation; keep the bridge's render callback render-pass scoped.
4. Feed the merged effects through existing `FrameScheduler` drawable/recovery behavior so the App remains free of terminal-specific deadline plumbing.
5. Cover the visible lifecycle through runtime/winit tests and a bridge-level scenario.

## Blocked by

- T0001 — provides the Frame Demand contract and reset semantics.

## Blocks

- None.

## Acceptance

- [ ] With a drawable, idle widget-hosted terminal, Runtime returns deadline-driven redraw work and the cursor alternates visibility across phases.
- [ ] Routed terminal input or a cursor move while hidden causes the next widget frame to show the cursor immediately and restarts blinking.
- [ ] No blink-only continuous `Poll` occurs; idle control flow waits until the next deadline.
- [ ] A non-drawable surface suppresses blink wakes, and becoming drawable causes one frame followed by resumed deadlines.
- [ ] Terminal rendering stays inside the widget-managed RenderPass and never performs direct surface acquisition, submission, or presentation.

## Out of Scope

- Public direct winit/wgpu terminal hosting; covered by T0003.
- Generalizing external scheduling to unrelated custom renderers.
- Changes to PTY invalidation, paste confirmation, or terminal input-routing policy.

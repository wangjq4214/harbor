# Main Window Runtime Presentation

**Ticket ID:** T0005
**Source:** [Spec: 0004-widget-runtime-host-simplification](../../spec/0004-widget-runtime-host-simplification.md)
**Status:** Todo

## Goal

A main-window redraw is acquired, rendered through Widget and Terminal CustomPaint, submitted, and presented entirely by runtime integration using resources borrowed from App.

## Layers

- [ ] **Runtime Host:** Build a borrowed frame target from the App-held Window and GPU context, invoke one integration entry point, and remove normal-path encoder/render-pass/submit/present orchestration.
- [ ] **Winit Runtime Integration:** Acquire the SurfaceTexture, create the view and encoder/pass, run the frame, submit commands, notify the window, present, and return a frame outcome.
- [ ] **Core Widget Runtime:** Update dirty state and encode Widget primitives, text, and external draws in retained paint order into the integration-owned pass.
- [ ] **Terminal / Application Components:** Expose the raw borrowed wgpu resources needed to construct the target without making `harbor-widget` depend on `harbor-terminal`; preserve transient CustomPaint GPU injection.
- [ ] **Verification:** Add integration coverage for frame ordering and borrowed-resource boundaries plus a manual main-window rendering demonstration.

## Approach

1. Add narrow borrowed accessors or target construction in the App-held GPU context for Window, Surface, Device, Queue, and current configuration; do not transfer ownership.
2. Implement the successful/suboptimal frame acquisition path in the winit integration and construct the render pass currently assembled by `App::render_frame`.
3. Invoke Runtime update and encode so quad, text, and Terminal CustomPaint draws retain their existing paint order, clip, and GPU context injection.
4. Submit the integration-owned command buffer, call pre-present notification, present the SurfaceTexture, and feed completion into the Runtime scheduler.
5. Return explicit outcomes to App for diagnostics and fatal policy rather than logging Harbor-specific decisions in Runtime.
6. Remove the App's normal successful-frame orchestration after frame-output parity is established.

## Blocked by

- T0001 — provides borrowed target and frame outcome contracts.
- T0004 — provides redraw/frame-completion scheduling consumed by presentation.

## Blocks

- T0006 — recovery extends this successful frame path.

## Acceptance

- [ ] The main terminal and Widget content render with unchanged ordering, clipping, text, and background.
- [ ] Runtime integration calls acquire, encode, `queue.submit`, pre-present notification, and `present` for a successful frame.
- [ ] App remains the long-lived owner of Window, Surface, Device, and Queue.
- [ ] Runtime retains no Window or Surface reference after the frame call returns.
- [ ] `harbor-widget` has no dependency on `harbor-terminal` or its `GpuContext` type.
- [ ] The App's successful redraw branch contains no encoder, render pass, submit, or present logic.
- [ ] Existing sub-2 ms encode target for a typical 80×24 frame is retained.

## Out of Scope

- Lost/outdated/timeout/occluded/validation recovery; covered by T0006.
- Device recreation or multiple GPU adapters.
- Confirmation-window presentation.
- Changing Terminal rendering pipelines or Widget primitives.

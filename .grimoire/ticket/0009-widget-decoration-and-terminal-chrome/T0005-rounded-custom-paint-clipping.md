# Rounded CustomPaint Clipping

**Ticket ID:** T0005
**Source:** [Spec: 0009-widget-decoration-and-terminal-chrome](../../spec/0009-widget-decoration-and-terminal-chrome.md)
**Status:** Todo

## Goal

A `CustomPaint` child inside a clipped `DecoratedBox` renders only inside the selected rounded shape while retaining its existing external draw, scheduling, and frame-appearance contracts.

## Layers

- [ ] **Widget API and Layout:** Compose the existing `DecoratedBox` and `CustomPaint` APIs without adding Terminal-specific properties or changing either allocation.
- [ ] **Fiber and Retained Scene:** Retain the active rounded child clip on `Primitive::External` SceneItems while preserving draw identity and paint order.
- [ ] **Widget Renderer and Frame Encoder:** Enforce hard-edge and anti-aliased rounded masks around the external callback, restore render state afterward, and retain rectangular surface-bound scissoring.
- [ ] **Runtime Host and Terminal Bridge:** Exercise a Terminal-compatible external draw handler through the existing bridge contract; do not change Terminal ownership, callback signatures, or Host policy.
- [ ] **Verification:** Add synthetic external-draw integration tests and bridge regression coverage for clipping, state restoration, eligibility, scheduling, appearance, and zero-size allocations.

## Approach

1. Ensure the active retained rounded clip reaches each `Primitive::External` item alongside its existing allocation and ancestor bounds.
2. Extend Frame Encoder handling around the callback so external pixels obey the rounded mask rather than only the rectangular `ExternalDrawContext::scissor_rect()`.
3. Preserve frame-scoped callback invocation, `ExternalDrawId`, `ExternalDrawMode`, scheduling eligibility, and frame-appearance behavior from ADRs 0005, 0011, 0015, and 0021.
4. Restore scissor, mask, pipeline-relevant state, and subsequent widget encoding after each external draw.
5. Keep zero-size and fully clipped external allocations callback-safe and free of invalid wgpu commands.
6. Demonstrate with an external handler that paints through corners, then verify clipped output and unchanged callback observations.

## Blocked by

- T0001 — Defines the retained rounded clip contract used by external SceneItems.
- T0004 — Provides working rounded clip propagation and renderer semantics for normal child content.

## Blocks

- T0006 — Terminal cannot safely receive the product anti-aliased radius until its external pixels obey the clip.

## Acceptance

- [ ] `Primitive::External` pixels outside a `HardEdge` rounded clip are absent.
- [ ] `Primitive::External` pixels at an `AntiAlias` rounded edge receive smooth coverage consistent with normal widget children.
- [ ] The external callback still receives its existing allocation, surface geometry, mode, identifier, and scheduling behavior.
- [ ] The external callback is not invoked for empty or fully clipped allocations when the existing contract says no drawable work exists.
- [ ] Renderer state is restored so widgets before and after the external item paint correctly.
- [ ] Nested ancestor clips remain authoritative and external content cannot escape them.
- [ ] Terminal bridge input, focus, cursor scheduling, synchronized output, and frame appearance regressions remain green.

## Out of Scope

- Changing `Terminal`, `RenderTarget`, `ExternalDrawFn`, or external scheduling API ownership.
- Applying the final product preset in `src/app.rs`; covered by T0006.
- Standalone terminal adapter decoration.
- Arbitrary external path clipping or offscreen save-layer APIs.

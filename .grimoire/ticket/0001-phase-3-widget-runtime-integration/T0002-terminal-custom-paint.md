# Terminal CustomPaint

**Ticket ID:** T0002
**Source:** [Spec: 0001-phase-3-widget-runtime-integration](../../spec/0001-phase-3-widget-runtime-integration.md)
**Status:** Todo

## Goal

The main Widget Runtime visibly hosts the existing terminal renderer as a focusable, clipped `CustomPaint` node and routes terminal input through the App provider.

## Layers

- [ ] **`harbor-text`:** Consume the foundation unchanged; no new CPU text behavior is needed for this slice.
- [ ] **`harbor-widget`:** Implement `CustomPaint`, `ExternalDrawId`, provider-by-ID encode/input contracts, batch flush/state restoration, and safe stale-ID behavior.
- [ ] **`harbor-render`:** Expose the existing terminal drawing and event handling through the App-owned external provider with the supplied rect and rectangular clip.
- [ ] **App host:** Replace the red demo root with terminal CustomPaint; translate applicable winit events, supply providers, pass actual viewport scale factor, and retain App ownership of submit/present.
- [ ] **Verification:** Cover provider ordering/input deferral and manually verify terminal output, clipping, focus, and idle redraw behavior.

## Approach

1. Define minimal external-draw and external-input provider contracts in `harbor-widget` that contain no terminal types.
2. Make `Primitive::External` executable: flush compatible Widget batches, set node clip/transform, call the provider, and restore Widget renderer state.
3. Make CustomPaint focusable and queue external input until Runtime propagation completes.
4. Adapt App and `UiRoot` so App-owned terminal rendering fulfills the provider call inside the main render pass.
5. Remove the demo scene and exercise resize/DPI viewport updates through the real root.

## Blocked by

- T0001 — Uses the migrated terminal text adapter and shared crate structure.

## Blocks

- T0003 — The native confirmation host extends this App/Runtime lifecycle and event routing.

## Acceptance

- [ ] The terminal is visibly rendered only through a main-root CustomPaint rect and respects its rectangular clip.
- [ ] Terminal pointer/keyboard events reach the existing App terminal path only after Runtime routing.
- [ ] An unknown or stale external ID does not panic or draw unrelated content.
- [ ] Widget batches before and after external drawing retain correct order and renderer state.
- [ ] Main-window rendering still uses one surface frame, encoder submission, and presentation per redraw.

## Out of Scope

- Widget-rendered labels or paste-confirmation UI.
- Secondary confirmation window creation.
- Cross-window modality and paste action handling.

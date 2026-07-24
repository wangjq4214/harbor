# Shared Text Foundation

**Ticket ID:** T0001
**Source:** [Spec: 0001-phase-3-widget-runtime-integration](../../spec/0001-phase-3-widget-runtime-integration.md)
**Status:** Todo

## Goal

Provide one shared CPU text contract that terminal and Widget renderers can use without changing visible terminal behavior.

## Layers

- [ ] **`harbor-text`:** Create the crate and move shared font discovery, fallback, glyph rasterization, atlas data, metrics, and text-run cache contracts into it.
- [ ] **`harbor-widget`:** Add the dependency and consume the shared text-facing types without yet exposing a user-visible text Widget.
- [ ] **`harbor-render`:** Replace private CPU text ownership with the shared core while retaining the terminal's existing GPU adapter and output.
- [ ] **App host:** Register the workspace crate and adapt font bootstrap imports without changing window behavior.
- [ ] **Verification:** Add CPU equivalence/cache tests and retain terminal-render regression coverage.

## Approach

1. Create `harbor-text` with only CPU-side ownership and no wgpu or winit dependency.
2. Move or extract the terminal's reusable font, glyph, metric, and cache responsibilities behind the new crate's types.
3. Adapt `harbor-render` to preserve its terminal-specific GPU atlas and rendering API while sourcing CPU data from `harbor-text`.
4. Add `harbor-widget` dependency wiring and narrow adapter interfaces needed by later Text primitives.
5. Verify shared caching and existing terminal metrics before removing duplicate CPU implementations.

## Blocked by

- None — pre-refactoring ticket.

## Blocks

- T0002 — Terminal CustomPaint needs the migrated terminal adapter.
- T0003 — Native confirmation labels require shared Widget text inputs.
- T0004 — Confirmation actions extend the shared-text window.
- T0005 — Preview text and wrapping use the shared metrics contract.

## Acceptance

- [ ] `harbor-text` is a workspace crate with no wgpu, winit, `harbor-render`, or `harbor-widget` dependency.
- [ ] Terminal rendering retains its existing glyph metrics and visible output under regression coverage.
- [ ] Repeated lookup of an already-cached `(font, glyph)` causes no additional CPU rasterization.
- [ ] `harbor-render` and `harbor-widget` compile against the shared CPU contract without a dependency from widget to render.

## Out of Scope

- Widget Text primitive rendering.
- `CustomPaint`, secondary surfaces, and paste-confirmation behavior.
- A shared GPU glyph atlas or GPU pipeline.

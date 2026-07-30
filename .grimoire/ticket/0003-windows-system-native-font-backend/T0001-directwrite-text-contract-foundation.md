# DirectWrite Text Contract Foundation

**Ticket ID:** T0001
**Source:** [Spec: 0003-windows-system-native-font-backend](../../spec/0003-windows-system-native-font-backend.md)
**Status:** In Progress

## Goal

A compile-tested shared contract exists for Windows-native font ownership, stable glyph identity, CPU atlas placement, and terminal consumption so every DirectWrite scenario can be added without redesigning these boundaries.

## Layers

- [ ] **Font Sources:** Represent a system-selected source and an optional `HARBOR_FONT` path as source descriptors; do not implement live source selection yet.
- [ ] **DirectWrite Backend:** Add the minimum Windows DirectWrite dependency features, a Windows-only backend module, native identity/ownership wrappers, backend errors, and an explicit non-Windows compile boundary.
- [ ] **Text Core & CPU Atlas:** Introduce backend-neutral face/glyph identity, font metrics, rasterized-glyph data, and an atlas key containing face, glyph, size, and style; adapt the current implementation through a temporary compatibility path so the workspace remains green.
- [ ] **Startup & Terminal Rendering:** Adapt `harbor-terminal` and startup ownership signatures to consume the new text contract without changing visible rendering; define a safe loader-thread/render-thread ownership model without manually asserting unsafe `Send`/`Sync`.
- [ ] **Verification & Profiling:** Add contract tests for identity equality/hash behavior, atlas separation, error propagation, ownership assumptions, and the explicit unsupported-platform diagnostic.

## Approach

1. Add only the `windows` crate features required by DirectWrite and place them under `cfg(windows)` for `harbor-text`; keep `wgpu`, `winit`, and `harbor-widget` out of the crate.
2. Define backend-neutral types for native face identity, resolved glyph identity, size/style keys, primary metrics, bitmap bounds, and grayscale bitmap data. Keep DirectWrite interface types private to the Windows module.
3. Introduce the internal backend boundary needed by selection, resolution, metrics, and rasterization, while preserving a narrow public façade for workspace consumers.
4. Re-key CPU atlas storage by stable glyph identity and keep a separate code-point-to-resolution mapping; provide a temporary adapter for existing rasterization until T0002–T0004 replace each behavior path.
5. Update `harbor-terminal` atlas lookup/upload call sites and `TextMetrics` construction to consume backend-neutral outputs while leaving the WGPU texture and draw pipeline unchanged.
6. Make the native-resource thread policy explicit: either prove the wrapped DirectWrite interfaces can safely move from the loader to the rendering owner or create them on their owning thread; never add unchecked thread-safety implementations.
7. Add compile/unit tests and focused documentation for the contract, including why char-only atlas identity is no longer sufficient.

## Blocked by

- (none)

## Blocks

- T0002 — Default DirectWrite selection and Latin rendering consume the shared contract.
- T0003 — Process-private configured fonts use the source and ownership abstractions.
- T0004 — System fallback requires stable face/glyph identity and resolution caching.
- T0005 — Cleanup and memory verification depend on every path using the new contract.

## Acceptance

- [ ] `harbor-text` has a Windows-only native backend boundary and a clear non-Windows unsupported diagnostic.
- [ ] Stable atlas identity distinguishes two faces with the same glyph ID and distinguishes size/style variants.
- [ ] `harbor-text` remains free of `wgpu`, `winit`, and `harbor-widget` dependencies.
- [ ] Existing terminal text still compiles and renders through the compatibility adapter with unchanged WGPU ownership.
- [ ] No native DirectWrite handle crosses threads through an unchecked `unsafe impl Send` or `unsafe impl Sync`.
- [ ] Focused contract tests, workspace formatting, and relevant crate tests pass.

## Out of Scope

- Selecting a real DirectWrite system primary face; covered by T0002.
- Loading `HARBOR_FONT`; covered by T0003.
- Calling DirectWrite system fallback; covered by T0004.
- Removing `fontdb`, `fontdue`, or the temporary compatibility adapter; covered by T0005.
- Changing atlas dimensions, GPU texture format, shaping, ligatures, or color glyph rendering.

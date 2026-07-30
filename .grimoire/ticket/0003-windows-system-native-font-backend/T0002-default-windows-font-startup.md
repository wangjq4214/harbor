# Default Windows Font Startup

**Ticket ID:** T0002
**Source:** [Spec: 0003-windows-system-native-font-backend](../../spec/0003-windows-system-native-font-backend.md)
**Status:** Todo

## Goal

With `HARBOR_FONT` unset, Harbor starts on Windows, derives terminal metrics from a DirectWrite-selected system primary face, and displays its first Latin frame through the existing WGPU atlas.

## Layers

- [ ] **Font Sources:** Query the Windows system font services when no environment override is present, without consulting Harbor's hard-coded path candidates.
- [ ] **DirectWrite Backend:** Create and own the DirectWrite factory/collection/primary face, expose primary metrics, resolve primary-face Latin glyph IDs, and rasterize them to grayscale bitmaps.
- [ ] **Text Core & CPU Atlas:** Build the public font façade from the native primary face, derive terminal metrics from that face, and place resolved Latin glyphs under stable atlas keys.
- [ ] **Startup & Terminal Rendering:** Wire default native loading through the current app startup path, construct `TextMetrics`, upload glyphs to the unchanged WGPU R8 atlas, and present the terminal frame.
- [ ] **Verification & Profiling:** Add Windows integration coverage for default loading, metrics, Latin rasterization, startup ownership, and a first-frame E2E/manual demonstration.

## Approach

1. Implement the no-override source policy with DirectWrite APIs and no Harbor-maintained candidate chain or filesystem scan.
2. Materialize only the native objects required for the chosen primary face; release discovery-only resources as soon as their result is retained.
3. Map DirectWrite design metrics to backend-neutral cell width, line height, ascent, and line-decoration inputs. Keep all terminal geometry primary-face based.
4. Resolve and rasterize primary-face Latin glyphs using DirectWrite glyph IDs and grayscale output compatible with the existing atlas texture.
5. Replace the default branch of `load_system_fonts` with the native backend façade while leaving configured-font and system-fallback behavior for their later tickets.
6. Ensure the app's font-loader thread and terminal rendering owner obey the resource policy established by T0001, adjusting initialization placement if native objects are not transferable.
7. Add a Windows test that loads, measures, rasterizes, atlas-packs, uploads, and visibly presents representative Latin text.

## Blocked by

- T0001 — Provides native ownership, backend-neutral metrics/glyph types, stable atlas identity, and terminal contracts.

## Blocks

- T0003 — The configured-primary branch extends this working DirectWrite primary path.
- T0004 — System fallback extends primary glyph resolution and rasterization.
- T0005 — Final legacy removal requires a complete default native path.

## Acceptance

- [ ] With `HARBOR_FONT` absent, Harbor reaches first presentation using a DirectWrite-selected primary face.
- [ ] Primary cell width, line height, ascent, underline, and strikethrough inputs are positive and stable across repeated queries.
- [ ] Representative Latin text and spaces render correctly through the existing WGPU R8 atlas.
- [ ] The default startup branch does not invoke hard-coded font paths, `fontdb`, `fs::read`, or `fontdue`.
- [ ] Discovery-only DirectWrite resources are released after primary selection; only required native state remains live.
- [ ] Windows integration tests and existing terminal rendering tests pass.

## Out of Scope

- `HARBOR_FONT` success and failure behavior; covered by T0003.
- Missing-character system fallback, CJK, symbols, and emoji; covered by T0004.
- Removing legacy code and Cargo dependencies that remain for incomplete branches; covered by T0005.
- Preserving the exact font family chosen by the former hard-coded candidate list.
- Complex shaping, ligatures, color glyphs, or GPU atlas redesign.

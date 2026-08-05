# System Fallback and Stable Glyph Cache

**Ticket ID:** T0004
**Source:** [Spec: 0003-windows-system-native-font-backend](../../spec/0003-windows-system-native-font-backend.md)
**Status:** Todo

## Goal

Characters missing from either the system-selected or configured primary face render through cached DirectWrite system fallback without changing terminal cell geometry or corrupting atlas entries across faces.

## Layers

- [ ] **Font Sources:** Use the DirectWrite system fallback service for missing Unicode scalars from both primary-source policies; do not scan Harbor candidate paths.
- [ ] **DirectWrite Backend:** Map missing characters to fallback faces/glyph IDs, rasterize native fallback glyphs, retain only used faces, and cache successful and unavailable resolutions.
- [ ] **Text Core & CPU Atlas:** Maintain separate character-resolution and face-aware glyph caches, key atlas entries by face/glyph/size/style, and keep primary metrics authoritative.
- [ ] **Startup & Terminal Rendering:** Resolve new terminal characters on demand, upload only newly rasterized atlas regions, rebuild safely when required, and display mixed Latin/CJK/symbol text through the unchanged WGPU pipeline.
- [ ] **Verification & Profiling:** Demonstrate first and repeated fallback, configured-primary fallback, multiple fallback faces, unavailable-result caching, stable metrics, redraw, resize, and DPI behavior.

## Approach

1. Implement the minimal DirectWrite text-analysis source required to call system fallback mapping for the requested Unicode scalar or text segment, carrying the primary family/style and locale policy explicitly.
2. Convert each mapping result into stable native face and glyph identities; retain only faces actually selected for rendered characters.
3. Cache code-point/style resolution results, including a deterministic unavailable result, so repeated redraws never remap the same request.
4. Rasterize mapped glyph IDs through the same grayscale DirectWrite path used for primary glyphs and return backend-neutral bitmap bounds.
5. Store rasterized output by face/glyph/size/style identity while preserving a character-to-resolved-glyph lookup for terminal cells; prevent collisions when equal glyph IDs come from different faces.
6. Keep `TextMetrics` immutable and primary-derived regardless of fallback advance widths or bounds; terminal wide-cell behavior remains owned by the grid.
7. Cover system-selected and `HARBOR_FONT` primaries with CJK, symbols, available grayscale emoji outlines, repeated misses, atlas rebuild/eviction, and GPU incremental upload tests.

## Blocked by

- T0001 — Provides stable face/glyph identity, resolution cache, and atlas contracts.
- T0002 — Provides the native primary and grayscale rasterization path.
- T0003 — Provides the configured-primary path required by fallback E2E coverage.

## Blocks

- T0005 — All missing-glyph behavior must be native before legacy fallback code can be deleted and profiling can be final.

## Acceptance

- [ ] A CJK character absent from the primary face resolves through DirectWrite and appears in the terminal atlas.
- [ ] Missing characters from a configured `HARBOR_FONT` face also use DirectWrite system fallback.
- [ ] Repeated rendering and redraw of a resolved character performs one fallback mapping per resolution-cache key.
- [ ] Repeated rendering of an unavailable character reuses a cached unavailable result and performs no repeated system scan.
- [ ] Glyphs from different native faces cannot collide even when their glyph IDs are equal.
- [ ] Fallback rendering leaves cell width, line height, ascent, underline, and strikethrough metrics unchanged.
- [ ] Mixed-script rendering survives incremental upload, full atlas rebuild/eviction, resize, and DPI-change tests.
- [ ] No hard-coded CJK candidate, representative CJK probe, `fontdb`, or full-file parser is invoked by fallback resolution.

## Out of Scope

- Complex-script shaping, grapheme clusters, bidi, and ligatures.
- Full-color emoji or changing the R8 atlas texture format.
- Asynchronous fallback, placeholders, cancellation, and completion-triggered redraw.
- Dynamic atlas sizing or a new atlas eviction algorithm.
- Non-Windows fallback backends.

# Process-Private Font Override

**Ticket ID:** T0003
**Source:** [Spec: 0003-windows-system-native-font-backend](../../spec/0003-windows-system-native-font-backend.md)
**Status:** Done

## Goal

A valid `HARBOR_FONT` file becomes Harbor's process-private primary face, while a missing, unreadable, or unsupported override produces a clear startup error without silent fallback.

## Layers

- [ ] **Font Sources:** Read only the `HARBOR_FONT` path value and define deterministic handling for ordinary font files and TTC collections without installing the font.
- [ ] **DirectWrite Backend:** Create a process-private font-file/face reference, retain only required native handles, and convert path, format, and face-selection failures into contextual errors.
- [ ] **Text Core & CPU Atlas:** Treat the configured face as primary, derive all terminal metrics from it, and rasterize its supported glyphs through the stable identity/atlas contract.
- [ ] **Startup & Terminal Rendering:** Propagate invalid override errors through `src/app.rs`; for valid overrides, render the first frame and preserve the existing WGPU upload/draw path.
- [ ] **Verification & Profiling:** Cover valid file, TTC first-face compatibility, missing file, unreadable/unsupported input, no global registration, and visible configured-font use.

## Approach

1. Preserve `HARBOR_FONT` as a filesystem-path override and keep it higher priority than system primary selection.
2. Use DirectWrite custom font-file/face APIs to create a process-private primary reference; do not add it to the user or system collection.
3. For a collection input, preserve the existing first-face behavior unless DirectWrite reports that face unusable; report a contextual error rather than silently selecting an unrelated system face.
4. Reuse the T0002 metrics and primary-glyph rasterization path with the configured native face.
5. Make an invalid explicit override a hard startup error carrying the path and native failure context; do not fall back to system default, legacy candidates, or `fontdb`.
6. Add Windows integration fixtures or controlled system-font copies that can verify primary selection without introducing an unlicensed distributable font.
7. Verify that process/system font collection state is unchanged before and after the configured font object's lifetime.

## Blocked by

- T0001 — Provides configured-source, ownership, error, glyph, and atlas contracts.
- T0002 — Provides a working DirectWrite primary metrics/rasterization path and startup integration.

## Blocks

- T0004 — Configured-primary fallback E2E coverage requires this override path.
- T0005 — Legacy configured loading cannot be removed until this path is complete.

## Acceptance

- [ ] A valid `HARBOR_FONT` path selects that file's first usable face as the primary face and renders its supported Latin glyphs.
- [ ] Primary terminal metrics come from the configured face.
- [ ] Missing, unreadable, malformed, and unsupported configured files produce a clear error containing the configured path.
- [ ] An invalid override never silently selects the Windows default or invokes a legacy loader.
- [ ] The configured font remains process-private and leaves the Windows user/system font collections unchanged.
- [ ] Destroying the font façade releases its custom native references.
- [ ] Windows override integration tests and app error-propagation tests pass.

## Out of Scope

- Installing or registering fonts globally or for the user session.
- Selecting a configured family name instead of a file path.
- System fallback for characters missing from the configured face; covered by T0004.
- Arbitrary UI for choosing a collection face.
- Legacy dependency removal and memory acceptance profiling; covered by T0005.

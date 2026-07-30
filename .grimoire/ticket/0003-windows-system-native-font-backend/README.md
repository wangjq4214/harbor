# Windows System-Native Font Backend

**Source:** [Spec: 0003-windows-system-native-font-backend.md](../../spec/0003-windows-system-native-font-backend.md)
**Ticket folder:** `.grimoire/ticket/0003-windows-system-native-font-backend/`

## Overview

Replace the Windows `fontdb`/`fontdue` path with a DirectWrite backend that owns font selection, process-private overrides, system fallback, metrics, glyph identity, and grayscale rasterization. `harbor-text` remains CPU-only, while `harbor-terminal` retains the WGPU atlas and rendering pipeline. The completed feature allocates no complete font-file copy in the Rust heap and rejects non-Windows builds explicitly.

## Layers

The project's architectural layers confirmed during decomposition are:

1. **Font Sources** — `HARBOR_FONT` and the Windows system font collection.
2. **DirectWrite Backend** — native resource ownership, primary selection, fallback, metrics, and glyph rasterization.
3. **Text Core & CPU Atlas** — `FontBook`, stable glyph identity, resolution caches, and `GlyphAtlas` placement.
4. **Startup & Terminal Rendering** — `src/app.rs`, `harbor-terminal`, WGPU atlas upload, display, and startup errors.
5. **Verification & Profiling** — Windows integration/E2E tests, DHAT, and private-memory measurement.

Every ticket includes all confirmed layers. The pre-refactoring ticket may have no externally visible result, but establishes the contracts consumed by every vertical slice.

## Dependency Graph

```text
T0001 → T0002 → T0003 → T0004 → T0005
```

### Blocking relationships

| Ticket | Blocks | Reason |
| --- | --- | --- |
| T0001 | T0002, T0003, T0004, T0005 | Every slice consumes the shared backend, glyph-identity, atlas, ownership, and platform contracts. |
| T0002 | T0003, T0004, T0005 | Override and fallback behavior extend the working default-primary DirectWrite path, and cleanup requires that path. |
| T0003 | T0004, T0005 | Configured-primary fallback tests consume the process-private override path; cleanup waits for override migration. |
| T0004 | T0005 | Legacy removal and final profiling require every primary and fallback behavior to use DirectWrite. |
| T0005 | — | Final cleanup and performance-verification slice. |

### Parallel groups

No safe parallel groups exist. T0002–T0005 successively modify the same `harbor-text` contracts and terminal call sites; parallel work would create file and runtime-contract conflicts.

## Recommended Order

1. T0001 — DirectWrite Text Contract Foundation
2. T0002 — Default Windows Font Startup
3. T0003 — Process-Private Font Override
4. T0004 — System Fallback and Stable Glyph Cache
5. T0005 — Legacy Removal and Memory Verification

## Ticket Index

| Ticket ID | File | Title | Summary |
| --- | --- | --- | --- |
| T0001 | [T0001-directwrite-text-contract-foundation.md](./T0001-directwrite-text-contract-foundation.md) | DirectWrite Text Contract Foundation | Establish shared native-backend, glyph-identity, atlas, ownership, and platform contracts. |
| T0002 | [T0002-default-windows-font-startup.md](./T0002-default-windows-font-startup.md) | Default Windows Font Startup | Render the first Latin terminal frame from a DirectWrite-selected system primary face. |
| T0003 | [T0003-process-private-font-override.md](./T0003-process-private-font-override.md) | Process-Private Font Override | Honor valid `HARBOR_FONT` files privately and report invalid overrides explicitly. |
| T0004 | [T0004-system-fallback-and-stable-glyph-cache.md](./T0004-system-fallback-and-stable-glyph-cache.md) | System Fallback and Stable Glyph Cache | Resolve and cache missing characters across native faces without changing terminal metrics. |
| T0005 | [T0005-legacy-removal-and-memory-verification.md](./T0005-legacy-removal-and-memory-verification.md) | Legacy Removal and Memory Verification | Remove legacy loaders and dependencies, then prove the memory target. |

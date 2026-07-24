# Widget Runtime Owns Independent Instanced Quad Pipeline

**Status:** Proposed
**Date:** 2025-07-23

## Context

Harbor's `harbor-render` crate already owns a `colored_quad_pipeline` (shared `Arc<wgpu::RenderPipeline>`) used by Background, Decoration, Selection, Cursor, and Scrollbar layers. The widget runtime could reuse this pipeline or create its own.

Two factors drive the decision:
1. The widget runtime design specifies an *instanced* quad pipeline (one draw call per batch of quads with instance data in a buffer), whereas the existing pipeline uses per-vertex color attributes with one draw call per quad.
2. Sharing the pipeline would create a dependency from `harbor-widget` on `harbor-render`, coupling the widget runtime to the terminal renderer's GPU abstractions.

## Decision

`harbor-widget` creates its own instanced quad pipeline internally. It does not reference or reuse `GpuContext::colored_quad_pipeline()`.

## Consequences

- Widget runtime owns its render pipeline end-to-end; no coordination with terminal renderer on pipeline layout changes.
- `harbor-widget` adds a direct `wgpu` workspace dependency but does NOT depend on `harbor-render`.
- Two quad pipelines coexist in the same process (terminal colored_quad + widget instanced_quad). The GPU driver handles pipeline switch efficiently; batch count is the binding constraint, not pipeline count.
- If rounded rect support is added later, the widget shader can evolve independently.

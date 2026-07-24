# `harbor-widget` Depends on `wgpu` but NOT `harbor-render`

**Status:** Proposed
**Date:** 2025-07-23

## Context

The widget runtime needs wgpu to create pipelines, vertex buffers, and encode draw calls. It must integrate with Harbor's existing frame encoding (shared Device/Queue/RenderPass). Two integration models exist:

1. Widget runtime depends on `harbor-render` and reuses its GPU abstractions (`GpuContext`, `Component` trait, vertex types).
2. Widget runtime depends only on `wgpu` and exposes its own encoding entry point; the binary crate wires them together at the frame level.

## Decision

`harbor-widget` depends on `wgpu` (workspace dependency) but NOT on `harbor-render`. The binary crate (`src/app.rs`) is responsible for passing the shared `wgpu::RenderPass` to both `harbor-render` components and `harbor-widget`'s `Runtime::encode()`.

## Consequences

- Clean separation: `harbor-widget` has no compile-time knowledge of terminal rendering, font atlases, or `GpuContext`.
- The binary crate gains the responsibility of orchestrating two independent renderers within one RenderPass. Paint order (terminal content → widget overlay) is enforced at the binary level.
- `harbor-widget`'s `Cargo.toml` adds `wgpu = { workspace = true }`.
- When Phase 3 introduces shared text (`harbor-text`), both crates will depend on that shared core, but `harbor-widget` still won't depend on `harbor-render`.

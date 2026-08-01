# Move Event Adaptation into the Widget Runtime Boundary

**Status:** Superseded
**Date:** 2026-07-25
**Superseded by:** [ADR 0015](./0015-runtime-owned-frame-presentation.md)

## Context

The binary crate currently converts winit events into widget events and coordinates runtime updates, which spreads UI pipeline responsibilities across `src/` and `harbor-widget`. Alternatives were to retain conversion in the binary, let the core Runtime depend directly on winit, or place a winit adapter beside a platform-independent Runtime.

## Decision

Keep the binary as a thin Runtime Host responsible for application entry, window and surface lifecycles, GPU submission and presentation, and cross-window policy. Move event conversion, IME handling, widget updates, layout, paint scheduling, and redraw decisions behind `harbor-widget` runtime APIs, using a feature-gated winit adapter so the core Runtime does not expose winit types; migrate behavior first and clean up APIs only after parity is established.

## Consequences

- `src/` retains winit `ApplicationHandler`, main and confirmation window coordination, surfaces, and presentation but no longer owns widget event translation.
- The runtime provides one coordinated input-to-scene lifecycle while the existing Widget Renderer remains the replaceable GPU drawing layer.
- Platform-independent runtime behavior can be tested without constructing winit events.
- The adapter introduces an explicit optional winit integration boundary in `harbor-widget`.
- Migration requires parity tests before legacy translation and orchestration code can be removed.

# Separate `harbor-widget` Crate from `harbor-ui`

**Status:** Proposed
**Date:** 2025-07-16

## Context

The existing `harbor-ui` crate contains a single-purpose modal dialog system with independent winit window lifecycle. The widget runtime design proposes a fundamentally different architecture — a retained-mode declarative UI runtime that encodes into the host frame without owning a window or surface. Merging the new runtime into `harbor-ui` would create a confusing dual-mode crate.

## Decision

Create a new `harbor-widget` crate for the widget runtime. Keep `harbor-ui` unchanged until Phase 3, when the terminal is embedded via `CustomPaint` and the old paste-confirmation window can be removed.

## Consequences

- Clean separation: `harbor-widget` never depends on winit; `harbor-ui` continues working during migration.
- Two UI crates coexist temporarily; `harbor-ui` will be deprecated and removed in Phase 3.
- No shared types between the two crates during the transition period.

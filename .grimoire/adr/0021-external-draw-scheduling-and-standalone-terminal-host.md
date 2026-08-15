# External Draw Scheduling and Standalone Terminal Host

**Status:** Implementing
**Date:** 2026-09-11

## Context

`CustomPaint` external draws presently participate in widget frame encoding but need to contribute timer-driven work before the widget scheduler selects its idle control flow. Terminal must also be usable directly with winit and wgpu; alternatives were App-mediated terminal deadline plumbing, a widget-only terminal implementation, or independent blink logic in each host.

## Decision

Extend the external draw contract with a scheduling callback that the Runtime gathers before idle and merges into its own redraw and `WaitUntil` policy. Provide a feature-gated public standalone terminal winit/wgpu adapter that consumes the same `Terminal Frame Demand`, while keeping the Terminal core free of window and surface ownership; suspend blink wakes when the surface is not drawable and request a frame when it becomes drawable again.

## Consequences

- The Runtime, rather than the App, owns widget-hosted terminal redraw scheduling.
- Direct and widget-hosted terminal paths use the same cursor timing and visibility-reset semantics.
- The standalone adapter owns its winit/wgpu host resources, while Terminal remains render-pass based.
- Tests must cover idle blink transitions, reset after input or cursor movement, and drawable-surface suspension and restoration in both hosting modes.

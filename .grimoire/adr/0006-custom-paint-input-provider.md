# CustomPaint Input Provider

**Status:** Superseded
**Superseded by:** [0009-app-cross-window-input-gate.md](./0009-app-cross-window-input-gate.md)
**Date:** 2026-07-24

## Context

The terminal must remain App-owned while becoming a focusable `CustomPaint` node whose input is routed by the Widget Runtime. Alternatives were leaving terminal input outside the Widget event path or forwarding terminal events through the same temporary App provider used for external drawing.

## Decision

A focusable terminal `CustomPaint` node queues external input by identifier during Widget event routing, and the Runtime delivers it to the App-supplied provider after propagation. A modal `FocusScope` intercepts events before they reach this bridge.

## Consequences

- The Runtime controls terminal focus, hit testing, and modal input isolation.
- The App continues to own terminal input handling and terminal model state.
- External input is delivered only after Widget routing has completed, preserving EventCtx's deferred-mutation boundary.

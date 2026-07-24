# CustomPaint Provider-by-ID Integration

**Status:** Proposed
**Date:** 2026-07-24

## Context

Phase 3 must place the existing terminal renderer inside the Widget Runtime paint order without making `harbor-widget` depend on terminal types or retaining mutable App state in a Fiber. The alternatives were a callback owned by `CustomPaint` and an App-supplied provider selected by a stable external-draw identifier.

## Decision

`CustomPaint` stores an `ExternalDrawId`, and `Runtime::encode` invokes an App-supplied external-draw provider for that identifier after flushing widget batches and applying the node's transform and clip. The App retains ownership of the terminal renderer and encodes it through this provider.

## Consequences

- `harbor-widget` remains independent of terminal and App types.
- Terminal drawing obeys Widget layout, clipping, and paint order.
- The integration avoids retaining App borrows or requiring shared mutable ownership in the Fiber tree.

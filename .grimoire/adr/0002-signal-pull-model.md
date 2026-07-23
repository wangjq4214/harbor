# Pull-Based Dirty-Flag Signal Model

**Status:** Proposed
**Date:** 2025-07-16

## Context

The widget runtime needs fine-grained reactivity: writing to a Signal should only invalidate fibers that read from it. Two models exist: push-based (Signal directly notifies subscribers, e.g. Leptos) and pull-based (Signal increments a version; fibers compare versions during build, e.g. Flutter's ChangeNotifier).

## Decision

Use a pull-based dirty-flag model. During fiber build, the runtime records which Signals are read. On Signal write, the runtime marks subscribing fibers with `BUILD_DIRTY`. The actual reconciliation happens later during `Runtime::update()`, where dirty fibers re-read their Signals and compare versions.

## Consequences

- Aligns with the design document's explicit dirty-flag taxonomy (`BUILD_DIRTY`, `LAYOUT_DIRTY`, `PAINT_DIRTY`, `HIT_TEST_DIRTY`).
- Batching: multiple Signal writes within one event loop tick are coalesced into a single rebuild pass.
- Requires a thread-local or runtime-scoped dependency tracking context during build.
- Signal reads outside a build context (e.g., in event handlers) do not create subscriptions — only reads during reconciliation are tracked.

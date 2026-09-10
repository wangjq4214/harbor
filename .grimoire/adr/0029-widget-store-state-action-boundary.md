# Widget Store State/Action Boundary

**Status:** Completed
**Date:** 2026-09-10

## Context

Product widgets need a reusable way to receive Host-owned declarative state and submit one-shot user intent without exposing application code to shared queue implementation details. The existing Tab UI assembled a `Signal<TabUiState>` and `Arc<Mutex<VecDeque<TabCommandRequest>>>` directly, while the Runtime Host remained responsible for reducing commands and applying terminal, PTY, focus, window, and GPU effects.

Moving reducers or effect execution into `harbor-widget` would couple the generic Runtime to Harbor application policy. Treating one-shot actions as persistent signal state would also blur their distinct delivery semantics. Background producers additionally require explicit Runtime invalidation, which a plain action queue cannot provide.

## Decision

Provide `Store<S, A>` in `harbor-widget` as a composition of a UI-thread `Signal<S>` and a private FIFO action inbox. The Host publishes state with `set_state` and drains actions at its chosen event-turn boundary. Components subscribe with `watch(BuildCx)` and receive only a cloneable, write-only `Dispatcher<A>` for submitting actions.

`Store` and `Dispatcher` do not own reducers, execute effects, or automatically wake a Runtime. Poisoned action-inbox locks are recovered deterministically so queued and future user intent remains accessible. Harbor's Runtime Host continues to own application models, action reduction, external invalidation, and platform effects.

## Consequences

- Product widgets no longer depend on the inbox container or locking implementation.
- State publication continues to use the existing pull-based dirty-fiber Signal model and coalescing behavior.
- Actions preserve FIFO, drain-once semantics and remain distinct from persistent state.
- `Dispatcher` is the minimal capability captured by `Send + Sync + 'static` widget callbacks; the full Store and its UI-thread Signal are not presented as cross-thread state.
- Background producers must continue to pair work with the existing App event or external invalidation path.
- The Host's widget dispatch, RuntimeEffects application, action drain, reduction, and Host-effect ordering remains explicit outside `harbor-widget`.

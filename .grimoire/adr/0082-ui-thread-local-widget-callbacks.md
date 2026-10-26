# UI-Thread-Local Widget Callbacks

**Status:** Proposed
**Date:** 2026-10-23
**Partially supersedes:** [ADR 0029](./0029-widget-store-state-action-boundary.md), only its consequence tying ordinary widget callbacks to `Send + Sync` and consequently restricting their captures to Dispatcher.
**Related:** [ADR 0081](./0081-preserve-click-naming-and-standardize-disabled-builders.md).

## Context

The historical UI/API review F3 identifies a mismatch between UI-thread Signal ownership (`Rc<RefCell<_>>`) and Button, IconButton and MouseRegion callback bounds (`Send + Sync`). InteractiveRegion and text-input callbacks already accept local captures. Source inspection additionally found the same unnecessary restriction on IconButton's `color_with` and Notification's dismissal callback; these run in UI content construction and event handling respectively.

The user approved the UI-thread callback scope, including these additional callbacks. The accepted direction removes an artificial threading constraint without moving business effects into widgets. Public naming follows ADR 0081 rather than the review's proposed renaming.

## Decision

1. **Ordinary UI callbacks are local.** Use `Fn(...) + 'static` without `Send + Sync`, with `Rc` for shared callback storage. The selected affected interfaces are Button/IconButton `on_click`, MouseRegion `on_enter`/`on_exit`, IconButton `color_with`, and Notification dismissal. Existing local InteractiveRegion/text-input callbacks remain local. Parameter types and callback timing are preserved.
2. **Do not extrapolate locality across real thread boundaries.** External rendering, scheduling, host and background-work interfaces retain their established ownership and threading contracts. This is not a global `Arc`-to-`Rc` conversion.
3. **Local UI state and business intent remain distinct.** Local callbacks may capture Signal or other UI-local state directly. Application commands still use Dispatcher and Host-owned event-turn reduction/effects under ADR 0029. Removing callback bounds does not authorize replacing business intent transport with direct reducer/platform calls.
4. **Preserve routing and lifetime semantics.** Reuse the existing InteractiveRegion state machine, nested-control first refusal, capture/release, disabled checks and keyboard activation timing. Closure replacement and teardown continue following the established reconciliation and HMR lifecycle; local storage must not extend stale callback lifetimes.

## Consequences

- A button can update local reactive state without a command inbox solely to satisfy a thread bound.
- Application call-site simplification can remove demonstrated threading workarounds, but must preserve genuine cross-thread/shared-host coordination, business command ordering, focus and visible behavior.
- `Fn` and `'static` remain deliberate constraints. This decision does not grant implicit clone, borrowed non-static captures, `FnMut`, or runtime cross-thread access to Signal.
- ADR 0029's Store/Dispatcher ownership, FIFO delivery, Host reduction and explicit wakeup policies remain applicable. Only its callback-bound rationale is revised.
- This records an approved direction. Implementation, test acceptance and native/HMR verification are not established by this record.

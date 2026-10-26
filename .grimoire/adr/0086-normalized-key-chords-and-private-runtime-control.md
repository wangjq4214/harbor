# Normalized Key Chords and Private Runtime Control

**Status:** Completed
**Date:** 2026-10-25

## Context

The historical UI/API review U3 groups F10 and F16: public `KeyChord` fields allow values that bypass constructor normalization; public `View::new` accepts built-in widgets with arbitrary children despite its private `AnyView` bound; public Signal subscription methods require a hidden Runtime scope. These interfaces permit compilable operations that violate construction or subscription expectations.

The refinement conversation approved one U3 spec, direct API tightening without compatibility entry points, and preservation of the existing first-match rule within a single shortcut map. This records the selected direction, not completed implementation or acceptance.

## Decision

1. **Normalization belongs to the chord value.** Make `KeyChord`'s `key` and `modifiers` fields private, construct through `KeyChord::new`, and expose read-only `key()` and `modifiers()` accessors. Preserve ASCII lowercase normalization for `Character` and `NumpadCharacter`, complete modifier equality, and the distinction between ordinary and keypad keys. Do not introduce Unicode case folding. Constructor-enforced values keep equality, hashing, configuration conflict checking, display and routing consistent; normalizing only inside `Shortcuts::bind` would leave those other consumers exposed to inconsistent values.
2. **Preserve duplicate semantics at the correct layer.** Within one `Shortcuts` instance, the first matching binding wins, as in the current ordered lookup. Document and test that rule rather than changing it to replacement. Harbor application configuration continues rejecting duplicate chord ownership under [ADR 0040](./0040-application-command-dispatch-and-keybinding-precedence.md) and [ADR 0041](./0041-structured-keybinding-configuration.md). The widget-level rule is not permission to accept conflicting application configuration.
3. **Raw concrete View construction is internal.** Make `View::new` crate-private and retain private `AnyView`. Public composition uses `Component`, `IntoChildView` and the existing child attachment protocol; application-owned primitive leaves use `View::leaf`. Do not add an equivalent public arbitrary-children constructor. Preserve reconciliation key/type identity and the construction/lifecycle boundaries in [ADR 0079](./0079-defer-root-construction-and-unify-content-slot-boundaries.md) and [ADR 0080](./0080-unify-child-cardinality-and-replacement-semantics.md).
4. **Runtime-owned subscription bookkeeping is internal.** Make `Signal::subscribe` and `Signal::unsubscribe` crate-private. Public dependency tracking remains `BuildCx::track` or `Store::watch`, with hook state through `BuildCx::use_state`. Preserve dependency replacement, unmount cleanup and Runtime isolation. This narrows access without replacing the [pull-based Signal model](./0002-signal-pull-model.md) or the [Store/Dispatcher boundary](./0029-widget-store-state-action-boundary.md). Do not expose RuntimeId, manual subscription tokens or automatic wakeup as part of U3.
5. **Migrate directly, without scope expansion.** Migrate repository consumers, external-crate tests and affected examples to the supported APIs; do not retain compatibility entry points for the removed access. U4 naming changes, including `Shortcuts` to `ShortcutMap`, remain separate. Read-only diagnostic access such as `Runtime::arena` remains available.

## Consequences

- Struct literals, direct chord field access, external `View::new` and external naked Signal subscription calls intentionally stop compiling. Supported composition and dependency-tracking paths must remain usable from another crate.
- Shortcut equality/hash semantics and input normalization retain their existing meaning; duplicate widget bindings retain first-match behavior rather than becoming a new map-update API.
- The runtime remains responsible for subscription ownership and cleanup; external users are not given a capability that appears valid outside its hidden scope.
- Compile-contract checks and mounted routing/lifecycle regressions are required. Merely changing visibility or migrating tests to internal access does not establish acceptance.

# Normalized Key Chords and Private Runtime Control

**Spec ID:** 0040
**Status:** Implemented
**Date:** 2026-10-25
**Sources:** [UI API review](../../UI_API_REVIEW.md), U3 / F10 / F16; the refinement conversation approving private normalized KeyChord values, internal View/Signal control APIs, direct migration without compatibility entry points, first-match duplicate widget bindings, and a single spec without ticket slicing; [ADR 0086](../adr/0086-normalized-key-chords-and-private-runtime-control.md).

## Requirements

### R1. A KeyChord is normalized by construction

- `KeyChord::new` is the public construction path. Both `key` and `modifiers` fields are private; external callers cannot use struct literals or direct field mutation to bypass normalization.
- Public read-only `key()` and `modifiers()` accessors expose the normalized key and complete modifier set.
- Preserve existing ASCII lowercase normalization for `Key::Character` and `Key::NumpadCharacter`. Constructing either variant from ASCII `A` or `a` yields equal chords with equal hash behavior when modifiers match.
- Preserve non-ASCII characters without Unicode case folding, non-character key identity, complete modifier equality, and the distinction between ordinary and keypad variants. Normalization must not erase Shift or any other modifier.
- Binding lookup, input-side key normalization, application configuration conflict checking, and shortcut presentation must consume the same normalized value contract. Restricting normalization to the binding method is insufficient.

### R2. Duplicate policy stays explicit and layered

- Within one `Shortcuts` instance, the first matching binding wins. Repeated `.bind` calls for equal normalized chords do not replace the earlier action or invoke multiple actions for one key-down.
- Preserve existing ancestor lookup and action routing, including `Consumed` versus `PassThrough`. U3 does not change nested shortcut precedence.
- Harbor application configuration continues rejecting repeated bindings and conflicting chord ownership with the existing atomic keybinding fallback policy. Widget-level first-match behavior does not relax application configuration validation; see [spec 0013](./0013-configurable-application-keybindings.md).
- Document the value normalization and widget duplicate rule where public callers discover `KeyChord` and `Shortcuts::bind`.

### R3. Raw View construction is not an external composition API

- `View::new` becomes crate-private, and `AnyView` remains private to the crate. Do not replace the removed access with another public arbitrary-children constructor or compatibility entry point.
- Public composition remains available through `Component`, `IntoChildView`, widget-owned child setters and the shared attachment protocol, including `view!`. `View::leaf` remains available for application-owned `PrimitiveLeaf` implementations.
- Preserve existing reconciliation key/type identity, deferred construction, hook/content-slot ownership, and single-child replacement/clearing/cardinality rules. This change must not require a public wrapper that changes identity merely to replace the removed constructor.
- Retain read-only diagnostics such as `Runtime::arena`; tightening construction does not authorize removing introspection.

### R4. Runtime owns raw Signal subscription bookkeeping

- `Signal::subscribe` and `Signal::unsubscribe` become crate-private without public compatibility substitutes.
- External components track an existing Signal through `BuildCx::track` or read tracked Store state through `Store::watch`; hook state continues through `BuildCx::use_state`.
- Preserve dependency replacement on rebuild, hook state retention, subscription cleanup on unmount, and isolation between Runtimes. Removing external access must not change dirty-fiber coalescing or subscription lifecycle behavior.
- Signal value access/publication remains available under its existing contract. This is not a new automatic read-tracking or automatic wakeup mechanism; event-handler reads do not acquire build subscriptions.
- Do not expose RuntimeId or introduce manual subscription tokens as part of this work. Runtime scope remains internal.

### R5. Direct consumer migration is part of the change

- Migrate affected production callers, external-crate integration tests, compile fixtures, and documentation/examples in the repository in the same change. Do not preserve the removed access through aliases or move public contract tests inside the crate merely to make them compile.
- Replace direct chord fields/literals with the constructor/accessors. Replace external `View::new` calls with supported composition and key assignment while retaining the behavior each test covers. Use supported build-context dependency tracking instead of external naked subscription calls wherever such calls are found.
- No U4 renaming is included: `Shortcuts`, its constructors and unrelated widget names retain their current names.

## Solution

Enforce chord normalization at the value boundary and narrow the two hidden-context control surfaces rather than redesigning routing, construction or reactivity. [ADR 0086](../adr/0086-normalized-key-chords-and-private-runtime-control.md) records the approved choices and rejected boundary-only normalization alternative.

### Seams

| Seam | Connects | Expects | Provides |
| --- | --- | --- | --- |
| Chord construction and inspection | Public callers / app configuration / palette ↔ KeyChord | Constructor inputs retain key variant and complete modifiers; accessors are read-only | Normalized values whose equality/hash/display inputs agree with routing |
| Binding resolution | Shortcuts ↔ event router and typed Actions handler | Canonical chord equality; existing ancestor selection and action result | First matching local binding; existing consumption/pass-through behavior |
| Public composition | External Component / macro / PrimitiveLeaf → View and reconciliation | Supported child protocol and existing key assignment | Opaque descriptions without public arbitrary-child injection; retained identity/lifecycle |
| Dependency tracking | External build logic → BuildCx / Store → internal Signal bookkeeping | Tracking occurs in a real mounted build context | Runtime-owned subscriptions, dirty rebuilds and lifecycle cleanup without exposing Runtime scope |

### Current source and migration surfaces

The reviewed source baseline is `9514fa9` (`fix(harbor-widget): honor event consumption`). These observations locate the change; they are not acceptance evidence:

- `crates/harbor-widget/src/widgets/shortcuts.rs` owns chord construction, binding storage and ordered first-match lookup.
- `crates/harbor-widget/src/runtime/event_router.rs` constructs normalized chords for lookup and reads normalized key fields for press ownership. Its existing routing behavior must survive accessor migration.
- `crates/harbor-app/src/command.rs` constructs chords and uses them as conflict-checking keys. `crates/harbor-app/src/palette.rs` reads chord fields for presentation; its comment currently attributes normalization to the resolver rather than the value boundary.
- `crates/harbor-widget/src/view.rs` owns `View::new`, `View::leaf`, `BuildCx::track` and hook tracking. `signal.rs`, `store.rs` and `fiber/reconcile.rs` carry the internal subscription and cleanup seams.
- `crates/harbor-widget/tests/component_boundaries.rs` contains a chord literal; `tests/integration.rs` uses external `View::new`, including identity assertions. These tests need genuine public-path migration, not deletion of their assertions.
- U1's `FocusScope`, `CustomPaint` and `Portal` already use optional single slots and `into_single` in this baseline, and `tests/child_attachment.rs` contains corresponding coverage. Their existing contract is the construction precondition for U3; this spec neither reimplements U1 nor claims its runtime acceptance is complete.

## End-to-End Tests

### Canonical chords reach the mounted action

- **Given:** A mounted public `Actions`/`Shortcuts` composition with a focused descendant and a binding built with ASCII uppercase `Character` input.
- **When:** Uppercase and lowercase logical key-down events with the same complete modifier set are routed in separate presses.
- **Then:** Both resolve to the same bound action once per triggering key-down; a consumed action does not reach the descendant. A different modifier set does not match that binding. A binding explicitly returning `PassThrough` retains existing descendant delivery.
- Repeat the normalization case for `NumpadCharacter`; an ordinary character binding must not accidentally match the keypad variant or vice versa.

### Duplicate widget bindings do not redefine application policy

- **Given:** A mounted shortcut instance with two actions bound in order to equal normalized chords, such as constructor inputs `A` and `a` with identical modifiers.
- **When:** A matching key-down is routed.
- **Then:** Only the first action is selected. Existing nested-map precedence is unchanged.
- **Given:** Application configuration claiming an equal chord more than once within a command or across commands.
- **When:** The configuration is resolved.
- **Then:** Existing validation rejects the override set and retains atomic fallback, rather than allowing widget registration order to choose a winner.

### Supported external composition retains identity and cardinality

- **Given:** A separate-crate consumer composing a keyed built-in wrapper through supported child APIs, plus an application-defined `PrimitiveLeaf` through `View::leaf`.
- **When:** The tree mounts, rebuilds with the same parent-local key/type, and replaces or clears a single child through the existing protocol.
- **Then:** Supported code compiles; keyed identity/state retention and child replacement/cleanup remain unchanged. Invalid child counts continue to fail under the existing static/dynamic construction contract rather than succeeding through raw `View::new`.

### Dependencies follow the mounted Fiber lifetime

- **Given:** A mounted component tracking an external Signal through `BuildCx::track` (and a separate Store consumer through `Store::watch`), with observable build counts.
- **When:** A tracked value changes and `Runtime::update` runs.
- **Then:** Its consumer rebuilds under the existing coalescing behavior.
- **When:** The component rebuilds to track a different Signal, or the subtree unmounts, and the old Signal changes afterward.
- **Then:** The old dependency no longer schedules the replaced/unmounted consumer. Hook-backed state remains retained on ordinary rebuilds.
- **Given:** Two Runtimes subscribing through supported build paths, including a shared Signal.
- **When:** Values change and Runtimes update independently; one subscriber is subsequently removed.
- **Then:** Updates reach the correct surviving subscriptions without cross-Runtime identity collisions or cleanup removing the other Runtime's subscription.

## Test Plan

| Boundary | Required evidence |
| --- | --- |
| Removed public access | Separate-crate compile failures for KeyChord literals, direct field reads/writes, external `View::new`, and external Signal subscribe/unsubscribe. Diagnostics must reflect the intended access restriction rather than an unrelated setup failure. |
| Supported public access | Separate-crate compile success for constructor/accessors, Component/IntoChildView and macro composition, View::leaf, and build-context Signal/Store tracking. |
| Value invariants | Equality/hash and accessor checks for ASCII case pairs, unchanged non-ASCII input, complete modifiers, non-character keys, and ordinary/keypad distinction. |
| Mounted behavior | Routing and dependency-lifecycle scenarios above, plus retained child-cardinality and key/type identity assertions after test migration. |
| Consumer compatibility | Affected widget, macro and application tests compile and pass; user-facing examples use supported APIs, and palette shortcut labels preserve their existing meaning. |

Acceptance requires both compile-contract and mounted behavioral evidence. Pure unit equality tests cannot establish routing, and visibility-only checks cannot establish subscription cleanup. This document introduces no new GPU/native/HMR acceptance claim and does not discharge existing obligations in related specs.

## Decisions and Boundaries

- The user approved the proposed U3 contract and the single-spec route in this refinement conversation. [ADR 0086](../adr/0086-normalized-key-chords-and-private-runtime-control.md) preserves the durable API boundary decisions.
- [ADR 0079](../adr/0079-defer-root-construction-and-unify-content-slot-boundaries.md) and [ADR 0080](../adr/0080-unify-child-cardinality-and-replacement-semantics.md) supply the construction/identity invariants; [ADR 0002](../adr/0002-signal-pull-model.md) and [ADR 0029](../adr/0029-widget-store-state-action-boundary.md) supply reactive ownership and explicit wakeup boundaries.
- [ADR 0040](../adr/0040-application-command-dispatch-and-keybinding-precedence.md), [ADR 0041](../adr/0041-structured-keybinding-configuration.md) and spec 0013 retain application conflict validation and command dispatch semantics. This spec defines no new persistence format.
- No unresolved behavioral assumption is used to authorize a new API. Test harness placement is an implementation choice, provided the public compile checks genuinely run outside the crate.

## Out of Scope

- U4 naming/configuration migrations, including `Shortcuts` to `ShortcutMap`.
- Unicode case folding, changed modifier semantics, ordinary/keypad merging, replacement-style duplicate bindings, or changed nested shortcut precedence.
- Public AnyView, an alternative arbitrary-children View factory, public RuntimeId or manual subscription tokens.
- A new Signal/reactivity architecture, implicit event-handler subscriptions, automatic wakeup, or changes to Host drain/event ordering.
- Implementing other review units, changing read-only diagnostic access, or claiming new native/GPU/HMR validation.

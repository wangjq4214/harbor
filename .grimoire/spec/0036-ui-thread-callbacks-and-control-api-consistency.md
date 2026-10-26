# UI-Thread Callbacks and Control API Consistency

**Spec ID:** 0036
**Status:** Accepted (owner-confirmed)
**Date:** 2026-10-23
**Sources:** `UI_API_REVIEW.md` F3/P0; refinement conversation accepting retained `on_click`, disabled-by-builder/default-enabled controls, TextField/EditableText alignment, UI-local callback coverage including color/dismissal callbacks, Button/IconButton focus handles, no old editor `enabled` aliases, and simpler upper-layer calls; [ADR 0081](../adr/0081-preserve-click-naming-and-standardize-disabled-builders.md); [ADR 0082](../adr/0082-ui-thread-local-widget-callbacks.md).

This contract deliberately revises the review's proposed `on_activate`/`enabled` renaming direction.

## Requirements

### R1 — Ordinary UI callbacks accept local captures

The following callbacks accept `Fn(...) + 'static`, without `Send` or `Sync`, and use `Rc` for shared callback storage:

| Surface | Preserved callback signature, excluding storage |
| --- | --- |
| Button / IconButton `on_click` | `Fn(&mut EventCtx) + 'static` |
| MouseRegion `on_enter` / `on_exit` | `Fn(&mut EventCtx) + 'static` |
| IconButton `color_with` | `Fn(&InteractionState) -> Color + 'static` |
| Notification dismissal callback | `Fn() + 'static` |

They must accept direct captures of Signal and other UI-local `Rc` state. Existing InteractiveRegion/content and text-input callbacks retain their local model and parameter/timing semantics. Keep `Fn` and `'static`; this is not a move to `FnMut`, implicit clone or borrowed captures.

Do not mechanically remove thread bounds or replace `Arc` on external rendering, scheduling, host, background-work or genuinely shared-state interfaces. Signal remains UI-thread-owned. Local callbacks may submit business intent through Dispatcher; reducers, platform effects, FIFO delivery and explicit runtime wakeup remain Host responsibilities under [ADR 0029](../adr/0029-widget-store-state-action-boundary.md), as qualified by ADR 0082.

### R2 — Keep click naming and align disabled setters

- Button and IconButton retain `.on_click(...)`. This denotes logical activation through their existing mouse and keyboard paths, not mouse-only delivery.
- InteractiveRegion retains its existing `.on_activate(...)`. MouseRegion keeps boundary-specific `on_enter`/`on_exit`; text-input events keep their existing distinct names. Do not introduce aliases just to make these names identical.
- Button, IconButton and InteractiveRegion retain `.disabled(bool)`; TextField and EditableText replace their public `.enabled(bool)` with `.disabled(bool)`.
- These controls are enabled by default (`disabled == false`). `disabled(true)` disables; `disabled(false)` enables. Existing editor `.enabled(value)` consumers migrate to `.disabled(!value)` without changing behavior or expression evaluation.
- Do not retain the old TextField/EditableText `enabled` methods as compatibility aliases. No unrelated API, such as the Focus wrapper's own enablement setting, is renamed merely because it has the same spelling.
- Read-only remains separate from disabled. Preserve current editing, focus, selection and copy behavior for the corresponding states; renaming must not disable read-only text selection/copy or make disabled controls interactive.

Internal flags such as `InteractionState.disabled` need no polarity change. The requirement is a consistent public control builder, not a wholesale internal boolean rename.

### R3 — Forward stable focus identity through button APIs

Button and IconButton expose `.focus_handle(FocusHandle)` and forward it to their existing InteractiveRegion. Reuse the current focus machinery: assigning an identity must not add a focus wrapper, another logical Tab stop or a new Fiber solely for forwarding. Existing text-input/InteractiveRegion focus-handle behavior remains unchanged.

A supplied handle must identify the actual interactive control for programmatic focus restoration and normal keyboard-visible focus. Default behavior, disabled eligibility, traversal order and nested control focus remain unchanged.

### R4 — Preserve interaction and callback lifetime

Continue using the shared InteractiveRegion state machine rather than copying input handling into Button/IconButton. Preserve:

- Descendant first refusal: activating a nested close button does not press or activate its parent row.
- Existing mouse down/capture, move/re-entry, inside/outside release, cancel and capture release behavior; disabling a pressed control releases its capture through the existing rebuild path.
- Existing Enter/Space KeyDown activation timing and current repeat behavior. A callback is not dispatched a second time by a newly added wrapper or an additional click/activation route.
- Disabled controls do not activate; enabled-state changes do not silently change command ordering or focus behavior.
- Callback replacement on rebuild uses the current closure; removed/replaced callbacks are released when their existing owners are released. Unmount and supported HMR teardown must not retain stale callbacks or change old-generation release ordering.

No layout, content measurement, hit-box, theme, paint or accessibility-label changes are authorized solely by this callback/API migration.

### R5 — Simplify affected upper-layer calls, not just signatures

Review and migrate affected application composition, examples, documentation and tests together with the widget APIs. The user-visible goal is ordinary direct callback configuration, not a new wrapper layer around the old restrictions.

- Remove demonstrated upper-layer scaffolding whose only purpose was satisfying ordinary UI callback thread bounds: unnecessary local-state synchronization adapters, forwarding-only closures/helpers or state/intent intermediaries must be removed when their lack of semantic responsibility is established. Do not replace them with a new generic callback abstraction solely for this migration.
- Use the button's own `focus_handle` where an outer adapter exists only to assign that identity. Preserve wrappers that also supply traversal ordering, focus policy or other behavior.
- Migrate availability calls in place; do not require callers to manage both positive and negative forms or manually route configuration through an exposed InteractiveRegion.
- Keep application business commands on Dispatcher and preserve source/generation qualifications, FIFO effect ordering and post-routing Host execution. Thread-bound removal is not a reason to replace business commands with direct Signal/model/platform mutations.
- Do not indiscriminately remove explicit clones, application visual helpers or `Arc`/atomic state shared with a host. Preserve ownership, repeated-build semantics, confirmation coordination, permissions and genuine thread boundaries.

Relevant current surfaces include `crates/harbor-app/src/tab_view_ui.rs`, `confirmation.rs`, `workspace.rs` and `palette.rs`, plus notification composition and widget examples/docs. Inspect each affected site; this list does not assert that every helper or synchronized value in those files is redundant. Where a site already has a direct, purposeful call, leave it direct rather than forcing churn to meet a line-count target.

`docs/widget/learn.md` must no longer claim that Button requires `Send + Sync` or that Dispatcher is necessary for local button state. Provide a minimal direct local-Signal button example while retaining the distinct business-intent example and its Host drain/wakeup explanation. Catalog/signature examples and affected callers must agree with R1–R3.

## Solution and Seams

Use the existing UI-local callback model and InteractiveRegion as the behavioral authority. Button/IconButton remain their present control compositions; focus identities and callbacks flow through them without additional state machines or public callback layers. TextField delegates the same disabled meaning to EditableText. Upper-layer simplification is behavior-preserving cleanup within these seams, not a component or command-boundary redesign.

| Seam | Connects | Expects | Provides |
| --- | --- | --- | --- |
| UI closure ownership | Application/local state → widget callbacks | `Fn` plus explicit `'static` ownership; UI-thread invocation | Direct local captures, shared `Rc` callback lifetime, unchanged payload/timing |
| Control configuration | Button/IconButton/TextField → existing region/editor | One disabled polarity and stable optional focus identity | Same interactivity and focus target without forwarding-only caller wrappers |
| Routed input | Runtime → InteractiveRegion → callback | Existing nested routing, capture and keyboard rules | No duplicate activation or parent activation from a handled child |
| Business intent | UI callback → Dispatcher → Host | Existing source/generation qualification, FIFO and event-turn ordering | Commands still reduced/applied by the Host, not the widget callback |
| Lifecycle | Reconciliation/HMR → callback owners | Existing build/replace/unmount and generation-release ordering | New closure after replacement; no stale mounted callback after removal |

[ADR 0079](../adr/0079-defer-root-construction-and-unify-content-slot-boundaries.md)'s deferred construction/content-slot ownership and [ADR 0080](../adr/0080-unify-child-cardinality-and-replacement-semantics.md)'s child contracts remain unchanged.

## End-to-End Tests

### Local state works through actual controls
- **Given:** A mounted component whose UI-local Signal drives a displayed count and whose Button/IconButton click callbacks capture that Signal directly.
- **When:** Mouse activation and focused Enter/Space input are routed through the runtime.
- **Then:** The expected local count and rendered text update through normal invalidation; each existing activation path invokes the callback without extra dispatch. No action queue or synchronization adapter is required solely to make the capture compile.
- **Also:** Exercise MouseRegion enter/exit, IconButton color resolution and Notification dismissal with UI-local captures through their actual event/build paths. Boundary callbacks retain edge behavior, color changes use normal rebuilds, and dismissal follows its existing trigger.

### Disabled polarity and editor behavior are preserved
- **Given:** Equivalent pre-migration availability values, default controls, read-only text, and disabled text fields/editors.
- **When:** Defaults are used or `.disabled(value)` changes across rebuilds, followed by focus, keyboard, pointer, selection and copy operations as applicable.
- **Then:** Default controls remain interactive; disabled controls do not activate/edit. Migrated `enabled(x)` behavior matches `disabled(!x)`; read-only remains selectable/copyable under the existing editor contract. Existing pressed-control disablement releases capture. External compile fixtures reject TextField/EditableText `.enabled(...)` and accept the new methods.

### Button focus handles do not duplicate focus
- **Given:** Button and IconButton with stable handles among existing focusable controls.
- **When:** Tab/Shift+Tab traversal and handle-based focus restoration run before and after rebuilds.
- **Then:** Each enabled button contributes its existing single logical stop, the handle targets its interactive region, keyboard-visible focus and Enter/Space activation work, and disabled/nested behavior is preserved.

### Nested commands and upper-layer cleanup stay equivalent
- **Given:** Tab rows with close controls, search/palette controls carrying current source/generation qualifications, and application confirmation/notification composition.
- **When:** Affected controls are activated before and after their call-site migration/simplification.
- **Then:** Closing a child does not activate its parent; business intent payloads, queue count/order, focus destination and resulting Host effects are unchanged. Search retains generation guards and editor/IME ownership; confirmation/notification retain their respective permission and dismissal coordination. Geometry, scenes and hit regions do not change from this migration.

### Callback replacement and removal release captures
- **Given:** Local callbacks with observable captures installed in mounted controls.
- **When:** A rebuild replaces callbacks, then removes the controls and releases remaining external owners.
- **Then:** Later input uses only the new mounted callback, removed controls receive no input, and old captures are released under existing ownership rules. Supported HMR acceptance additionally checks old-generation callback release before unloading, not merely successful closure compilation.

## Decisions

- **Retain button `on_click`; align editors to `disabled`.** Explicitly selected by the user instead of the review's proposed rename; ADR 0081.
- **Cover all identified ordinary UI callbacks, including color/dismissal; preserve real cross-thread contracts.** Explicitly accepted scope; ADR 0082. This partially supersedes only ADR 0029's callback-bound rationale, not its state/action boundary.
- **Add button focus-handle forwarding and remove old editor enablement aliases.** Explicitly accepted in the refinement scope; do not infer a broader API renaming campaign.
- **Simplify upper-layer calls in the same change.** User's additional request, constrained by the review's preserved behavior and existing business/widget ownership. No particular helper is presumed redundant without evidence.

## Test Plan

- Add public-API coverage proving each R1 callback accepts non-Send/non-Sync captures; exercise direct local-Signal updates via mounted controls, not just isolated closure assignment.
- Extend widget interaction/focus tests and relevant application workspace/tab/confirmation/notification tests for R2–R5. Retain boundary-edge, nested-close, disable-during-capture, keyboard, focus restoration, theme and lifecycle regressions.
- Check callback replacement/removal with observable local capture lifetimes; native, IME and actual old-generation teardown evidence is distinct from mounted/headless coverage.
- Review public API/docs examples and upper-layer changes for removed workarounds and retained command/host responsibilities. A signature-only migration leaving proven thread-bound scaffolding or false documentation does not satisfy R5.

## Out of Scope and Limitations

- F4 composable Button/content-measurement refactoring, F5 layout extension helpers, and F6–F8 constructor/import, validation and named-parameter redesigns.
- Changing the macro DSL, F1/F2 construction contracts, business reducers, event buses, host scheduling or real cross-thread ownership.
- Renaming InteractiveRegion activation or unrelated Focus enablement APIs; implicit clone, `FnMut` callbacks or non-static borrowed closures.

No blocking semantic decision remains for this spec. Implementation may choose private storage/forwarding details within the fixed `Rc`, naming, focus and lifecycle contracts. The exact amount of upper-layer deletion depends on demonstrated redundancy; this contract does not authorize inventing new abstractions or changing behavior to make calls shorter.

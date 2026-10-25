# Return-Driven Event Consumption and Commit

**Spec ID:** 0039
**Status:** Draft
**Date:** 2026-10-25
**Sources:** [UI_API_REVIEW.md](../../UI_API_REVIEW.md) U2, F11 and F12; refinement conversation selecting return-value-controlled propagation, removal of public `stop_propagation()`, and a spec-only endpoint; [ADR 0085](../adr/0085-return-driven-event-consumption-and-unconditional-commit.md).

This contract describes required behavior, not completed implementation or acceptance. It covers U2 only; no ticket set or implementation plan is produced.

## Requirements

### R1 — One public consumption protocol

- `EventHandled::Handled` consumes the current routed event. Once that handler returns, no remaining handler/node/phase in that event walk receives it.
- `EventHandled::Ignored` allows propagation to continue through the existing routing order. It does not mean the handler performed no work or queued no effects.
- The contract applies consistently to built-in handlers, external `PrimitiveLeaf::handle_event` implementations, and `CustomPaint::on_input` callbacks. Returning Handled must suffice without a second operation.
- Remove public `EventCtx::stop_propagation()`. There must not remain a second public stop flag or compatibility entry point requiring callers to coordinate it with the returned result. Internal routing representation is not prescribed.
- Consumption and redraw are independent: a Handled result does not by itself request paint, and requesting paint does not by itself consume input.

Source: ADR 0085 decisions 1–2 and 5; F11.

### R2 — Phase semantics and nested control priority

- Retain capture → target → bubble ordering. Ignored results do not add new delivery phases or retarget the event to lower overlapping siblings.
- A Handled result in capture deliberately intercepts the event before later ancestors, the target and bubble. A Handled result at target prevents bubble; a Handled result in bubble prevents remaining bubble recipients. Already executed handlers are not undone.
- Ordinary enclosing activation controls continue to give descendants first refusal: a consuming inner control must not also press or activate its enclosing clickable control. Existing capture-phase observation must not become eager parent activation.
- Preserve intentional capture interception, including the ScrollView overlay scrollbar owning its track before descendant controls. Do not obtain child-first behavior by globally disabling capture handlers.

Source: ADR 0085 decision 4; the current InteractiveRegion capture guard and ScrollViewport scrollbar interception in the source inventory below.

### R3 — Notification callbacks are not a second consumption API

- Activation callbacks such as `on_click` report the owning control's activation. The control, not a void notification callback, decides whether the underlying input was consumed.
- Boundary-observation callbacks such as `on_enter`/`on_exit` do not cancel other nodes' required boundary notifications. Do not mechanically change all void callbacks into EventHandled-returning callbacks.
- A result applies only to its own dispatch. A Handled direct Enter/Leave or focus notification must not cancel another required notification or the subsequent ordinary pointer Move.
- Preserve hover transition order: Leave is delivered from the old inner nodes outward, followed by Enter from the new outer nodes inward, skipping the common path. Direct notifications remain distinct deliveries rather than one consumable ancestor walk.
- Preserve separately required focus-loss, focus-gain and focus-visibility notifications. A consumed notification must not leave another recipient with stale focus state.
- If source review exposes a callback that genuinely needs a propagation decision but cannot express one through its current input-handler return, resolve that API requirement before changing its signature; do not restore hidden context-based stopping or invent a new callback contract during implementation.

Source: ADR 0085 decisions 3–4; existing `update_hover_path` and focus-transition delivery.

### R4 — Submit accumulated work regardless of consumption or redraw

- Every normally completed or consumption-shortened event walk must submit its accumulated work exactly once. A Handled result stops delivery, not submission, and does not roll back prior side effects.
- When hover transition handlers require paint, the current Move must still submit its own focus, pointer capture/release, clipboard, external-input and other supported event commands through their established processing boundaries. It must not wait for another Move or update to make the current event's work available.
- Combine redraw demand from the already delivered hover notifications and the current event, including when the current walk ends early through consumption. Neither branch's redraw demand may suppress execution of the other branch's submission.
- Preserve existing command order and effect-coalescing rules, including the order of hover notification submissions before the ordinary Move walk. Exactly-once processing is not a promise that every intermediate coalesced clipboard or focus value is separately exposed to the Host.
- Ignored handlers may also queue work; their accumulated effects must not be discarded merely because their result was Ignored.
- `Runtime::dispatch` continues to return the current dispatch's RuntimeEffects, and deferred external input remains available through the existing external-input drain. Previously pending runtime work retains its existing separate publication contract. No result is reinterpreted as an application Command Outcome.

Source: ADR 0085 decision 5; F12; existing `finish_event`, `Runtime::dispatch` and external-input drain contracts.

### R5 — Preserve input ownership and application boundaries

- Consuming one event must not automatically consume later Up, Cancel, KeyUp, repeat or IME events. Preserve the established pointer capture/release and explicit consumed-key/IME ownership rules; this requirement does not remove existing deliberate suppression of a consumed shortcut's release.
- Preserve focused-target resolution, Tab/Shift+Tab traversal versus terminal forwarding, editor-local key priority, IME composition ownership, clipboard request ownership and stale-target protection. Return-driven propagation is not authority to redesign these policies.
- Keep `CustomPaint::on_input` as the in-tree adapter path: an installed callback does not also enqueue automatic deferred delivery. Without that callback, the existing queue-by-ExternalDrawId path remains available after routing. Already queued external input is not rolled back by consumption.
- Runtime effects, external-input delivery, application action draining and business effects retain their existing ownership and order. Cross-window paste safety remains application-owned under [ADR 0009](../adr/0009-app-cross-window-input-gate.md); Store/Dispatcher remain a FIFO business-intent boundary under [ADR 0029](../adr/0029-widget-store-state-action-boundary.md).

Source: ADR 0085 decisions 4 and 6; current event ownership, external-input and application bridge seams.

### R6 — Public API and consumer migration

- Migrate affected built-in handlers, repository application adapters, tests, public extension documentation and examples to the single return-driven contract. Do not leave examples telling extension authors to call both Handled and stop_propagation.
- Public extension examples must demonstrate a Handled result stopping propagation and an Ignored result permitting it without using crate-private routing APIs.
- Do not add consumption return values to notification callbacks merely to replace deleted stop calls. Check actual callback responsibilities; any needed new semantic decision returns to refinement as required by R3.
- Preserve callback ownership/thread constraints and established component construction/child contracts. This is not the broader widget-renaming migration from U4.

Source: ADR 0085 decisions 2–3 and 6 and its migration consequences; U2's extension-documentation scope.

## Solution

Use the existing EventHandled return channel as the sole public consumption decision, while keeping EventCtx for deferred side-effect requests. Retain the existing routing, direct-notification, RuntimeEffects and application bridge boundaries. The internal representation and organization of router helpers are implementation choices, not requirements of this spec.

### Source inventory

These are current surfaces to preserve or migrate, not claims that they already satisfy this contract:

| Surface | Relevant current responsibility |
| --- | --- |
| [`input/event_ctx.rs`](../../crates/harbor-widget/src/input/event_ctx.rs), [`view.rs`](../../crates/harbor-widget/src/view.rs) | EventHandled and EventCtx public contract; PrimitiveLeaf input extension |
| [`runtime/event_router.rs`](../../crates/harbor-widget/src/runtime/event_router.rs) | Routed phases, direct hover/focus notifications, ownership, command submission; currently discards handler returns and contains F12's short circuit |
| [`runtime/mod.rs`](../../crates/harbor-widget/src/runtime/mod.rs) | Public dispatch, RuntimeEffects publication and external-input drain |
| [`widgets/interactive_region.rs`](../../crates/harbor-widget/src/widgets/interactive_region.rs), [`widgets/scroll_view.rs`](../../crates/harbor-widget/src/widgets/scroll_view.rs) | Nested activation priority versus intentional scrollbar capture interception |
| [`widgets/editable_text.rs`](../../crates/harbor-widget/src/widgets/editable_text.rs), [`widgets/focus_scope.rs`](../../crates/harbor-widget/src/widgets/focus_scope.rs) | Editing, input lifecycle and focus navigation |
| [`widgets/custom_paint.rs`](../../crates/harbor-widget/src/widgets/custom_paint.rs), [`widgets/mouse_region.rs`](../../crates/harbor-widget/src/widgets/mouse_region.rs), [`widgets/notification.rs`](../../crates/harbor-widget/src/widgets/notification.rs) | External input adapters, boundary observations and notification-card input isolation |
| [`terminal_view.rs`](../../crates/harbor-app/src/terminal_view.rs), [`confirmation/preview.rs`](../../crates/harbor-app/src/confirmation/preview.rs) | Application PrimitiveLeaf/on_input consumers whose Handled returns acquire the selected propagation meaning |

### Seams

| Seam | Connects | Expects | Provides |
| --- | --- | --- | --- |
| Input handler | Built-in/PrimitiveLeaf/CustomPaint handler → router | UiEvent, EventCtx and the existing target geometry; returns EventHandled | Explicit continuation or consumption, without a public context stop operation |
| Deferred submission | Router → InputState and effect queues | Work accumulated by delivered handlers, including before consumption | Established ordered/coalesced effects processed once, independently of redraw demand |
| Direct notifications | Hover/focus state transitions → individual recipients | Required per-recipient notifications in their established order | Each recipient is updated; one result does not suppress another delivery |
| Runtime output | Runtime → Host/application bridge | Current-dispatch RuntimeEffects and queued external input through existing channels | No missing Move effects, no duplicate adapter delivery; Host retains platform/business responsibilities |

## End-to-End Tests

The scenarios below require mounted Runtime dispatch, not only direct calls to a leaf method. They describe required evidence; none was executed during spec creation.

### E1 — External leaf consumes a nested click (R1, R2, R4)

- **Given:** An InteractiveRegion with an activation counter contains a public PrimitiveLeaf. The leaf returns Handled for left Down/Up without calling a stop operation; enclosing activation handlers yield during capture.
- **When:** A pointer press and release occur inside the leaf.
- **Then:** The leaf receives the intended input, enclosing activation remains zero, and the leaf's valid accumulated effect requests are submitted. Repeat the consumption check with a CustomPaint on_input adapter. A built-in nested Button/IconButton must retain the same child-first activation behavior.

### E2 — Ignored means continuation, not absence of effects (R1, R2, R4)

- **Given:** A nested leaf queues a distinguishable clipboard or external-input effect but returns Ignored; the enclosing clickable control can handle the input.
- **When:** The input is dispatched along that path.
- **Then:** The enclosing control still handles it, while the leaf's work is retained according to existing coalescing rules. Handler traces follow the established phases rather than invoking an alternative overlapping sibling.

### E3 — Each phase can stop only the remaining walk (R1, R2, R4)

- **Given:** A mounted test tree records capture, target and bubble handler invocations and accumulates distinguishable effect requests.
- **When:** Separate cases return Handled in capture, at target, and in bubble; a control case returns Ignored throughout.
- **Then:** The invocation trace stops precisely after the consuming handler, or completes the existing order for the Ignored control. Earlier invocations and their queued work remain effective. Handled alone does not create a redraw request. Internal test fixtures may observe phases without adding a new public phase API.

### E4 — First hover Move still commits its own work (R3, R4)

- **Given:** A MouseRegion on_enter requests paint, and its target leaf writes `move-1` to the clipboard when handling the first ordinary Move.
- **When:** The pointer enters the region through that Move.
- **Then:** That same dispatch reports the clipboard write and redraw, without a second Move. No unhandled queued work survives to replay later. Exercise current-walk Handled and Ignored cases, and the corresponding Leave transition where another target handles the Move.
- **And:** Use supported valid focus/capture and queued external-input requests to cover the other submission channels, verifying their committed state/drain in the current dispatch turn. Include notification-originated and Move-originated work together, with observable counters/order and existing coalescing rather than only a redraw boolean.

### E5 — Independent notification delivery survives consumption (R2, R3, R4)

- **Given:** Nested boundary-aware nodes record Enter/Leave, and focus-aware nodes record a real focus transition. Some notification handlers return Handled and request repaint.
- **When:** The pointer crosses between paths, and focus moves between nodes.
- **Then:** All required boundary recipients are notified in existing Leave/Enter order, common-path recipients are not spuriously re-entered, ordinary Move still runs, and both old/new focus recipients receive their required notifications. Resulting redraw demand is retained even if the subsequent routed event is consumed without requesting paint.

### E6 — Capture remains useful and has a complete lifecycle (R2, R4, R5)

- **Given:** A ScrollView scrollbar overlays descendant controls, and another case has a leaf/control requesting pointer capture.
- **When:** A track/thumb interaction starts, moves outside the allocation and ends through Up or Cancel; also exercise removal/replacement of an active captor.
- **Then:** Existing scrollbar capture interception prevents unintended descendant activation, valid captured input follows its established recipient, release/cancellation clears capture, and no enclosing control is spuriously activated. Each new event follows established ownership rather than being discarded solely because an earlier event returned Handled.

### E7 — Keyboard, editor and terminal ownership remain intact (R1, R5)

- **Given:** Focused built-in controls, an editor, a terminal bridge and scoped shortcuts in representative application compositions.
- **When:** Dispatch activation keys, Tab/Shift+Tab, editor-owned keys, consumed-shortcut repeats/releases and IME preedit/commit across focus changes.
- **Then:** Existing focus traversal/terminal forwarding, editor priority, explicit consumed-key ownership and stale-composition protection remain intact; no parent is newly activated by consumed child input and no consumed event is delivered to a second adapter. Include clipboard completion after focus change/removal to preserve its existing stale-target rejection.

### E8 — External input and application actions are delivered once (R4, R5)

- **Given:** One CustomPaint uses on_input and another case uses deferred external input; an activation callback submits typed intent through Dispatcher.
- **When:** Input is routed and the existing Host/application consumption boundary is reached, including a hover-plus-Move case.
- **Then:** The in-tree callback is not also delivered through the automatic external queue, explicit queued input remains drainable in existing order, and typed intent retains FIFO/drain-once behavior. Consumption does not shift reducers or platform effects into widget handlers. Existing cross-window gating still prevents terminal delivery while paste confirmation owns input.

### E9 — Public API documents one protocol (R1, R6)

- **Given:** An external consumer implements PrimitiveLeaf or installs on_input using public types.
- **When:** Compile the consuming and pass-through examples and a negative API case calling EventCtx::stop_propagation.
- **Then:** Valid examples compile without private APIs, the removed method is not publicly callable, and reference documentation consistently describes return-driven consumption. Repository consumers compile without a replacement public stop flag or changed notification callback signatures introduced for mechanical convenience.

## Decisions

| Choice | Reason/source |
| --- | --- |
| EventHandled drives propagation; remove public stop_propagation | Confirmed refinement choice; ADR 0085 decisions 1–2. Avoid contradictory return/context instructions and make the existing extension contract meaningful. |
| Notification callbacks remain observations/activation reports | Confirmed refinement distinction; ADR 0085 decision 3. Void callbacks are not a reason to retain a duplicate propagation API. |
| Consumption is local and never rolls back accumulated commands | ADR 0085 decisions 4–5; U2/F12. Keep event ownership and submission independent of the redraw boolean. |
| Reuse the current router, not Web default-action cancellation or Flutter gesture arbitration | ADR 0085 decision 6. U2 fixes the existing model rather than replacing the input architecture. |
| One spec, no ticket slicing or implementation in this refinement | User selected the recommended spec-only endpoint. Both outcomes share the routing/submission contract. |

## Test Plan

- Characterize the relevant current routing, boundary and ownership cases before implementation changes; explicitly distinguish the F11/F12 bugs from behavior to preserve. The review's temporary probes are prior evidence, not permanent regression tests or results from this task.
- Require the public mounted regressions E1 and E4 in addition to internal phase traces. Direct handler-return unit tests alone cannot prove routing consumption or command submission.
- Use the existing input_routing, component_boundaries, composable_button, interaction_focus_actions_theme, external_input, runtime_effects, scroll_view, desktop_pointer_cursors and text_input suites as relevant regression surfaces, plus runtime-local phase/ownership tests and affected application tests. This is guidance for coverage, not a claim that all currently cover the new requirements.
- Verify affected public API/docs examples and repository application consumers compile after migration. Do not close the work by merely deleting stop calls or changing EventHandled documentation.
- Preserve applicable native focus/IME and actual HMR teardown verification obligations when changed input ownership/lifecycle paths affect them; mounted/headless tests do not establish native or DLL-unload acceptance. No performance threshold or new native capability is introduced by this contract.

## Out of Scope

- Flutter-style gesture recognition/arena arbitration or a Web-style browser default-action/preventDefault subsystem.
- Broad widget/module renaming, callback renaming, child-protocol redesign, host/GPU error reporting or renderer API work belonging to other review units.
- Changing application command meaning, cross-window paste policy, Store/Dispatcher reduction ownership, or established IME and key-sequence policy.
- Implementation, test execution, ticket creation or an implementation-plan stage during this spec-only task.

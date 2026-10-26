# Harbor Widget API Design Review

## 1. Current Conclusion and Baselines

**Continue converging the existing API rather than starting a generic API v2. U1–U3 fixes have landed; U4–U6 remain implementation units. Implementation status and full acceptance are recorded separately.**

- Status synchronization baseline: `3ea728c40719c576914242e1d6050f2a923f8e82`; original source-review baseline: `b5c41bb`.
- Keep F/U identifiers for ADR/spec references. Resolved findings retain summaries only; old probes, review procedures and historical test results are no longer current action items.
- P1 denotes prioritized contract/behavior risks; P2 denotes interface convergence or low-level entry-point improvements. Recommendations are neither evidence of an observed product failure nor implementation authorization.

## 2. Work Already Landed

| Unit / finding | Current implementation | Regression evidence entry points |
| --- | --- | --- |
| U1 / F2: Construction capabilities | FocusScope, CustomPaint and Portal use `Option<View>` and shared `into_single`; child setters replace, empty blocks clear, and excessive dynamic child counts return errors | `crates/harbor-widget/tests/{child_attachment,single_child_lifecycle}.rs`; commit `751e841` |
| U2 / F11, F12: Event consumption and commit | The router uses `EventHandled` to control propagation; public `stop_propagation()` is removed. The current event commits unconditionally before hover repaint flags are merged | `crates/harbor-widget/tests/event_consumption.rs`, `crates/harbor-widget/src/runtime/event_router/tests.rs`; commit `9514fa9` |
| U3 / F10, F16: Normalization and internal boundaries | KeyChord fields are private, normalized through construction and exposed through accessors; duplicate bindings retain first-match behavior. `View::new` and raw Signal subscriptions are crate-private | `crates/harbor-widget/tests/{key_chord,runtime_dependencies}.rs`, public-boundary fixtures under `crates/harbor-widget-macros/tests/ui/{fail,pass}/`; commit `3ea728c` |

Earlier findings F1/F3/F4/F5 are also no longer pending implementation: the macro and Runtime support deferred construction and existing View roots; content slots own independent Fibers; ordinary UI callbacks support UI-local captures; composable Button, shared ControlSurface, explicit layout components and fallible final-node macros already exist. The remaining native Host root-factory difference is tracked separately as F9.

Related contracts: ADR 0079–0084 / spec 0034–0038; for U2, see [ADR 0085](.grimoire/adr/0085-return-driven-event-consumption-and-unconditional-commit.md) / [spec 0039](.grimoire/spec/0039-return-driven-event-consumption-and-commit.md); for U3, see [ADR 0086](.grimoire/adr/0086-normalized-key-chords-and-private-runtime-control.md) and the [public compile/mounted regression entry points](#2-work-already-landed), which retain the retired spec 0040 contract.

**Acceptance boundary:** U2's ADR remains Testing and its spec remains Draft; U3's ADR remains Completed, with the former spec 0040 contract retained at the linked destinations. This synchronization does not change those statuses or deny existing implementation because older documents say spec-only. The tests above are regression evidence entry points, not results executed in this review; existing native focus, IME and real HMR generation-teardown verification obligations are listed in section 6.

## 3. Remaining Findings

Unless otherwise noted, source paths below are relative to `crates/harbor-widget/src/`.

### F6 / P2: Wrapper Construction, Imports and Naming Have Not Converged

**Design recommendation.** `widgets/{focus,shortcuts,actions}.rs` still use `empty/new(child)` and `handler/new(child, handler)`. They implement WithChildren but lack ordinary `.child(...)` setters. Focus's `.handle(...)` differs from controls' `.focus_handle(...)`.

Padding/Align/SizedBox are root-exported; Stack/TextLabel remain in submodules, Component/BuildCx/View are in the `view` module, and extension traits are at the root. Establish a recommended import policy; a prelude is only an option, not a defect in itself.

**Recommendation:** Constructors take only necessary configuration; add consistent child setters and align focus-configuration naming. See the full mapping in [section 7](#7-widget-and-method-naming-convergence-checklist-proposed-work). Do not export every internal type merely to reduce import lines or mechanically generate Props.

### F7 / P1: Remaining Appearance Configuration Lacks Predictable Validation Boundaries

**Statically confirmed; converge by domain rather than requiring every entry point to return Result.**

| Current entry point | Behavior / issue | Location |
| --- | --- | --- |
| `Icon::size` / `IconButton::icon_size` | Invalid values retain the previous value, making the call appear successful | `widgets/{icon,icon_button}.rs` |
| `TextField::padding` | Negative/non-finite values become zero, unlike external Padding's policy | `widgets/text_field.rs` |
| `Separator::thickness` | Stored unchanged; errors appear only during layout | `widgets/separator.rs` |
| `ControlStyle`, `Theme::spacing` | Public raw values flow directly into region/surface/text-and-icon layout without a unified validation entry point | `theme.rs`, `widgets/{control_surface,button}.rs` |
| `NotificationStyle` / placement inset | Style declares dimensional constraints without validating them; inset also accepts values unchanged | `widgets/notification.rs` |

**Recommendation:** Define legal domains and error policies for dimensions, colors and material caps, then share validation across equivalent entry points. Public input structs are not proof of validated configuration; consumption boundaries remain responsible. Preserve the distinction between handling invalid scalars at the boundary and handling parent constraints or overflow from valid-value combinations during layout. Do not redo migrated SizedBox/Padding/Align/Flex gap behavior or mechanically remove Backdrop performance caps or ScrollController request normalization.

### F8 / P2: Positional Parameters and Positioning Types Can Be Tightened

**Design recommendation.**

- `widgets/{editable_text,text_field}.rs`: true in `on_vertical_navigation(bool)` means down, and the application palette relies on that implicit convention. Recommend `VerticalDirection::{Up, Down}`.
- `widgets/backdrop_filter.rs`: tone `[offset,slope,quadratic,saturation]` and rim `[highlight,shade]` parameters can be swapped without compilation errors. Recommend local named configuration with shared validation; GPU uploads may retain compact arrays.
- `widgets/notification.rs`: placement and close alignment accept Stretch-capable `Alignment`, but only perform positioning, treating Stretch as Start. Use the existing `PositionAlignment/Alignment2D`.

Do not redo Insets/Align or remove Flex's genuine Stretch behavior.

### F9 / P2: Native Host Root Factories Still Reject Existing Views

**Statically confirmed.** `build/build_with_output` in `winit/host.rs` require `R: Component`, but View does not implement Component; `Runtime::set_root` already accepts IntoChildView. Macro Views and ordinary composition functions returning View still need extra wrappers to serve as Host factory results.

**Recommendation:** Both factories accept `IntoChildView + 'static`, preserving factory errors, bootstrap output and mounting timing. Add public compilation contracts for ordinary components, macro Views and existing Views without redesigning root identity or HMR.

### F13 / P1: Fatal GPU Errors Lack a Complete Production Reporting Path

**The wiring gap is statically confirmed; behavior under actual device failure remains unverified.** `winit/presenter.rs` exposes FrameError variants such as OutOfMemory/DeviceLost, and application `src/shell.rs` relies on HostFrameOutcome::Fatal to decide when to exit. However, `winit/gpu.rs` does not wire device-lost/uncaptured-error notifications after device creation, and encoding and submit/present lack complete error reporting. Existing classification mainly covers surface acquisition/recovery.

**Recommendation:** SharedGpu owns device-level error state and a wakeup mechanism, converting errors into explicit outcomes at frame/host boundaries. Explain asynchronous reporting timing and that submission/requested presentation does not mean completed GPU execution; do not promise synchronous capture of every error. Policy tests injecting FrameError cannot replace real production wiring, shared-device multi-window tests and device-failure verification.

### F14 / P2: QuadRenderer's Public Entry Points Lack a Draw-Order Contract

**Statically confirmed; a low-level API improvement recommendation, not evidence that Runtime ordering is currently wrong.** In `renderer/quad.rs`, update retains/reuses GPU slots and encode draws by slot range rather than SceneItem.paint_order. `runtime/frame_encoder.rs` already looks up slots in scene order and encodes segments.

An independent consumer calling `update(delta) → encode(pass)` directly may produce incorrect occlusion after insertion, reordering or slot reuse. Stable storage slots are not draw order, and slot-reuse tests do not prove correct occlusion.

**Recommendation:** Recommend Runtime's paint-ordered entry point. Make slot/range operations internal, or explicitly name them storage-order drawing and provide a complete ordered-items path. Preserve internal slot reuse and batching rather than rewriting the renderer.

### F15 / P2: Non-Drawable Windows Ignore host_deadline

**Statically confirmed; the scope of the deadline promise still needs clarification.** `winit/host.rs` accepts host_deadline and passes it through the presenter to the scheduler. The `!drawable` branch in `winit/scheduler.rs` returns Wait directly; only the drawable path arbitrates runtime/recovery/host deadlines.

If a caller uses this for non-rendering timers, a zero-size/minimized window with no other events will not wake on time. The current application still arbitrates business deadlines such as notifications itself, so this does not establish that all business timers have failed.

**Recommendation:** Suspend redraw/GPU animation while non-drawable but retain future host WaitUntil deadlines; alternatively, explicitly restrict/name the parameter as rendering timing and let callers arbitrate business deadlines. Add state tests for zero-size windows with/without future deadlines and restoration to drawable.

## 4. Pending Implementation Units

| Unit | Findings / order | Change scope and verification focus |
| --- | --- | --- |
| U4: Naming, named configuration and consistent entry points | F6, F7, F8; validation strategy precedes consumer migration | Configuration-owning modules, wrappers/exports, application, macro fixtures and docs; cover invalid-value matrices, equivalent layout/styles, parent constraints and business defaults. Naming checklist: section 7 |
| U5: Complete host and GPU outcomes | F9, F13, F15; the three subitems may proceed independently | host/gpu/presenter/scheduler, main/dialog, multi-window and HMR seams; cover component/View factory compilation, deadline arbitration, fault injection and native acceptance while preserving Host ownership, factory errors and teardown order |
| U6: Clarify public renderer layers | F14; no mandatory prerequisite | Renderer entry points/exports, independent consumers and docs; preserve internal reuse/batching and verify GPU results for reordering, removal/reinsertion and translucent overlap |

Settle the migration strategy before changing public contracts. Integrate shared-file and interdependent migrations serially or atomically. Do not reimplement U1–U3, but preserve their regression contracts in subsequent changes.

## 5. Boundaries Subsequent Changes Must Preserve

- Ordinary Rust expression macros, single evaluation/branch laziness/children-before-parent and explicit layout trees. No JSX, attribute DSL, generic layout method chains or universal Container.
- Independent content-slot Fibers, parent-local key/type, direct flex parent data and atomic layout commit. No promise of state preservation across arbitrary wrapper rewrites or moves between parents.
- Shared interaction state machine, private ControlSurface and distinct Button/IconButton theme roles. Preserve controller → editing core → decorated field layering.
- Store/Dispatcher FIFO and the Host drain/wakeup responsibility split; no automatic wake is an established contract.
- Host owns Window/Surface/Runtime; the application owns EventLoop, business logic, multi-window and exit policy. Preserve HMR generation gating, destruction barriers and native resource destruction order.
- Preserve bounded Lucide, borrowed GPU capabilities and application glass/command policy boundaries. UIA, virtual lists and full shaping are outside this backlog.

## 6. Verification and Synchronization Scope

- This synchronization updates only this document against current source, related ADR/specs and regression-test entry points. It is not another full source audit and changes no production code, tests or ADR/specs.
- This review did not rerun code tests or workspace checks; old probes, test counts and cached warnings are not current verification results.
- Formal regression tests exist for U1's mounted lifecycle, U2's event consumption/commit and U3's routing/subscriptions/public compilation boundaries. That does not establish acceptance of every requirement.
- Existing evidence gaps for native focus, IME, real HMR DLL unloading and generation teardown remain. Mounted/headless tests cannot replace that acceptance.
- U5/F13 still needs production device-error wiring, fault injection, multi-window and actual-device verification; U6/F14 still needs independent-entry draw-order and GPU occlusion verification.

## 7. Widget and Method Naming Convergence Checklist (Proposed Work)

This section belongs to U4 / F6 and accompanies F7/F8 validation and configuration migration. **The names below are proposed targets, not currently available APIs, final public signatures or completed implementation.** Do not rewrite component responsibilities merely for uniform names. Rename types only where names misrepresent responsibility; method naming should align setting, querying, dynamic configuration, events and appending.

### 7.1 Widget and Related Type Names

| Current name | Proposed name | Rationale |
| --- | --- | --- |
| `CustomPaint` | `ExternalSurface` | Handles external drawing, input, IME and scheduling, not just custom painting |
| `Notification` | `NotificationCard` | Expresses card presentation without implying notification lifecycle management |
| `NotificationStyle` | `NotificationCardStyle` | Follows the component name |
| `NotificationPlacement` | `NotificationCardPlacement` | Follows the component name; positioning-type tightening remains part of F8 |
| `Actions<A>` | `ActionHandler<A>` | Provides action handlers, not an action collection |
| `Shortcuts<A>` | `ShortcutMap<A>` | Makes the shortcut-to-action mapping explicit |
| `Alignment` | `CrossAxisAlignment` | Distinguishes Stretch-capable cross-axis layout from positioning-only types |

Before migrating `Alignment`, move positioning-only consumers such as Notification to PositionAlignment/Alignment2D. Do not mechanically replace every reference while continuing to expose Stretch at positioning entry points. Keep MainAxisAlignment, PositionAlignment, Alignment2D, FlexOptions, FocusHandle and Controller names; do not invent synonyms.

Recommended component names to retain:

- Layout: `Row`, `Column`, `Flex`, `Stack`, `Padding`, `Align`, `SizedBox`, `ConstrainedBox`, `Flexible`, `Expanded`, `Spacer`.
- Content and controls: `TextLabel`, `Icon`, `Button`, `IconButton`, `EditableText`, `TextField`, `Separator`.
- Wrappers and foundational mechanisms: `DecoratedBox`, `BackdropFilter`, `ScrollView`, `Focus`, `FocusScope`, `MouseRegion`, `InteractiveRegion`, `LayoutObserver`, `ThemeProvider`, `Portal`.

Their distinct responsibilities are meaningful. Do not add uniform Widget/Box suffixes, merge TextLabel/EditableText/TextField or rename Portal to imply a separate native window. Private ControlSurface remains an internal implementation.

### 7.2 Constructors and Content Methods

| Current API | Proposed API |
| --- | --- |
| `Focus::empty()` | `Focus::new()` |
| `Focus::new(child)` | `Focus::new().child(child)` |
| `Shortcuts::empty()` | `ShortcutMap::new()` |
| `Shortcuts::new(child)` | `ShortcutMap::new().child(child)` |
| `Actions::handler(handler)` | `ActionHandler::new(handler)` |
| `Actions::new(child, handler)` | `ActionHandler::new(handler).child(child)` |
| `Button::content(builder)` | `Button::child_with(builder)` |
| `InteractiveRegion::content(builder)` | `InteractiveRegion::child_with(builder)` |
| `LayoutObserver::from_callback(callback)` | `LayoutObserver::from_shared_callback(callback)` |

Shared conventions:

- `new` takes only necessary configuration, not a child. Required callbacks may still be configuration; do not force meaningless empty-callback defaults on observers or action handlers.
- `child` sets a single slot; `child_with` sets the same slot through a builder. They replace each other. The latter retains its independent Fiber, hooks and subscription lifecycle rather than executing inside the parent Fiber.
- `children` replaces a multi-child list; `push_child / extend_children` explicitly append. F2's empty-block clearing and count checks do not change with naming migration.
- `from_shared_callback` explicitly accepts a shared callback handle without broadening thread constraints.
- Retain meaningful convenience constructors such as `Button::text/icon`, `SizedBox::width/height`, `Separator::horizontal/vertical`, `ThemeProvider::from_signal` and Flex's `with_options`.

### 7.3 Property and Callback Methods

The table uses current type names to locate old entry points; implement it together with the type renames in 7.1.

| Current API | Proposed API | Semantic constraints |
| --- | --- | --- |
| `Focus::handle` | `focus_handle` | Consistent with ordinary controls |
| `Focus::order` | `tab_order` | Makes ordering purpose explicit |
| `Focus::enabled` | `focusable` | Focus eligibility only, not whole-control disabled state |
| `SizedBox::color` | `background` | Paints the box background |
| `TextLabel::color` | `foreground` | Content foreground |
| `Icon::color` | `foreground` | Icon foreground |
| `TextField::color` | `foreground` | Edited-text foreground, not every internal element's color |
| `EditableText::color` | `foreground` | Consistent with TextField |
| `IconButton::color_with` | `foreground_with` | State-driven icon foreground, not a surface-background override |
| `Notification::detail_color` | `detail_foreground` | Explicitly targets the detail section |
| `EditableText/TextField::on_changed` | `on_change` | Rename only; still reports only user-committed text changes |
| `EditableText/TextField::on_submitted` | `on_submit` | Preserve Enter modifiers and trigger timing |
| `CustomPaint::handler` | `ExternalSurface::on_draw` | Makes callback purpose explicit |
| `CustomPaint::schedule` | `ExternalSurface::schedule_with` | Scheduling-needs query provider, not an event notification |
| `CustomPaint::ime` | `ExternalSurface::ime_with` | IME state/geometry provider, consistent with dynamic resolvers |

Other conventions:

- Retain `disabled`, `selected`, `read_only` and `focusable`; do not merge distinct states into one switch.
- Retain `on_click`, `on_activate`, `on_enter`, `on_exit`, `on_escape`, `on_unmount` and `on_input`. The distinction between button on_click and generic-region on_activate follows ADR 0081; do not mechanically merge them.
- Retain dynamic evaluation for `max_height_with`. `*_with` accepts functions computing configuration or content, distinct from `on_*` event callbacks.
- Retain the name `on_vertical_navigation`; change its bool parameter to proposed `VerticalDirection::{Up, Down}`. Preserve IME priority and the existing vertical-navigation trigger scope; do not add horizontal navigation.
- Ordinary UI callbacks remain UI-local. External drawing/providers retain existing ownership and invocation timing; naming changes do not mechanically change Rc/Arc, Send/Sync or `'static`.

### 7.4 Query Methods

Retain `.property(value)` configuration builders; **Widget configuration queries consistently use `get_property()`**. This proposed style avoids Rust's lack of same-name getter/setter overloading rather than mixing `get_`, `_value` and unadorned names.

| Current API | Proposed API |
| --- | --- |
| `Align::get_insets` | Retain |
| `Padding::insets` | `get_insets` |
| `DecoratedBox::decoration` | `get_decoration` |
| `DecoratedBox::clip_behavior_value` | `get_clip_behavior` |
| `Row/Column/Flex::options` | `get_options` |
| `IconButton::label` | `get_label` |
| `SizedBox::requested_width/height` | `get_requested_width/height` |

Do not mechanically add get_ to controllers or read-only value types. Retain action verbs and plain property queries, but represent state accurately:

- Rename `ScrollController::offset()` to proposed `requested_offset()`, matching its request semantics: clamping awaits successful layout.
- Retain `ScrollMetrics::offset()` for the committed actual position.
- Do not uniformly rename `ScrollController::metrics/jump_to/scroll_by/reveal_range`, `TextEditingController` state queries/editing actions or immutable configuration-value accessors to match Widget builder style.

### 7.5 Migration and Verification Boundaries

1. Settle the final mapping and public behavior before migrating types/module exports, constructors, methods, application calls, macro fixtures and documentation examples. Coordinate module filenames with final type names; do not retain duplicate synonymous entry points or compatibility aliases long-term.
2. Preserve U1's implemented child protocol and coordinate it with U4's numeric/named configuration. Renaming is not a behavioral fix. Foreground does not broaden color scope, focusable does not become a blanket subtree-input disable, and requested scroll positions do not masquerade as committed positions.
3. Verify public API compilation, cross-crate/dependency-renamed macros, handwritten/macro declaration equivalence, child replacement/clearing, focus/input callback order and related documentation examples. Lifecycle behavior retains its existing native/IME/HMR acceptance obligations.

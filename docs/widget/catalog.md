# Widget catalog

[WIDGET home](README.md) · [Learn](learn.md) · [Recipes](recipes.md)

Browse by the job you need to do, then follow the source/coverage links. **Available** means the public implementation exists in this checkout. It does not mean stable API, feature parity with another toolkit, accessibility certification or a fresh native visual pass. See [testing and acceptance](testing.md).

## Lay out content

| Need | Available building blocks | Boundary / evidence |
| --- | --- | --- |
| Horizontal or vertical composition | `Row`, `Column`, `Flex`, `MainAxisAlignment`, `Alignment`, `.gap(...)` | Shared constrained flex engine, not CSS; [layout contract](../flex-layout.md), [flex runtime tests](../../crates/harbor-widget/tests/flex_runtime.rs) |
| Allocate flexible space | `Expanded`, `Flexible`, `FlexFit`, `Spacer` | Flex-parent relationships and bounded/unbounded constraints matter; [source](../../crates/harbor-widget/src/widgets/flexible.rs) |
| Impose size, insets or alignment | `SizedBox`, `ConstrainedBox`, `Padding`, `Align` | Logical pixels; `Align` can position overlay content without making its empty area hit-testable; [source](../../crates/harbor-widget/src/widgets/align.rs) |
| Layer ordinary children | `Stack` | Shared parent allocation, not a generic positioned/anchored popover engine; [source](../../crates/harbor-widget/src/widgets/stack.rs) |
| Scroll a single content subtree | `ScrollArea`, `ScrollController`, `ScrollMetrics` | Vertical scrolling and controller-visible geometry; not virtualized lists/trees/tables; [scroll tests](../../crates/harbor-widget/tests/scroll_area.rs) |
| Observe committed allocation | `LayoutObserver`, `LayoutChangedCallback` | Geometry notification, not application resize policy; [tests](../../crates/harbor-widget/tests/layout_observer.rs) |
| Add a rule | `Separator` | Layout/presentation primitive; [source](../../crates/harbor-widget/src/widgets/separator.rs) |

`SizedBox`, `Padding`, `Align` and `Stack` are imported through `harbor_widget::widgets::<module>::Type`, rather than all being crate-root exports. Check [public exports](../../crates/harbor-widget/src/lib.rs) instead of guessing import paths from type names.

## Display and activate

| Need | Available control | Boundary / evidence |
| --- | --- | --- |
| Single-line text | `widgets::text_label::TextLabel`, `.color(...)`, `.ellipsis()` | Monospace label, not rich text or a general shaping/layout engine; [source/tests](../../crates/harbor-widget/src/widgets/text_label.rs) |
| Built-in vector artwork | `Icon`, `LucideIcon` | Built-in Lucide inventory, not arbitrary SVG loading; [icon tests](../../crates/harbor-widget/tests/icon.rs) |
| Labelled activation | `Button` | `.on_click`, `.disabled`, `.selected`; uses the shared interaction state machine; [source](../../crates/harbor-widget/src/widgets/button.rs) |
| Icon activation | `IconButton` | Icon plus label and control styling; the label is not proof of a native accessibility bridge; [tests](../../crates/harbor-widget/tests/icon_button.rs) |
| Compose custom visuals with activation | `InteractiveRegion`, `InteractionState`, `ControlStyle` | Hover/press/focus/disabled/selected state and `.content(...)`/`.child(...)`; not a gesture-recognition suite; [source](../../crates/harbor-widget/src/widgets/interactive_region.rs) |
| Pointer-specific response | `MouseRegion` | Lower-level pointer hooks/cursor behavior; use an existing control when activation is the goal; [source](../../crates/harbor-widget/src/widgets/mouse_region.rs) |
| Transient dismissible message | `Notification` | A reusable message composition, not a global notification center, timer service or OS toast API; [source](../../crates/harbor-widget/src/widgets/notification.rs) |

Button activation callbacks require `Send + Sync`; use a `Dispatcher` for application intent rather than capturing a UI-thread signal. See [Learn step 3](learn.md).

## Edit a single line

| Layer | Available API | Ownership |
| --- | --- | --- |
| Coherent editing state | `TextEditingController`, `TextEditingValue`, `TextSelection` | Text, selection/caret, composition, revision, history and acceptance predicate |
| Decoration-free editor | `EditableText` | Editing input, focus, caret/selection geometry, horizontal scrolling and clipping |
| Decorated field | `TextField` | Composes that same editor with padding and `BoxDecoration`; placeholder, enabled and read-only modes |

Use `TextField::new(controller.clone())`, not a parallel application string editor. It is already focusable: assign `.focus_handle(handle)` directly instead of adding another focus stop.

Current behavior includes grapheme-safe navigation/replacement/deletion, selection drag, local Windows editing chords, undo/redo and tagged clipboard completion. Public positions are **UTF-8 byte offsets normalized to extended grapheme boundaries**, not character counts. `on_changed` reports user committed-text changes only; programmatic replacement, selection movement and preedit do not fire it. `on_submitted` receives Enter modifiers; `on_escape` leaves cancellation policy to the consumer.

This is single-line plain text. There is no multiline/password/rich-text/form system promise. Native candidate positioning, mixed-Unicode visuals and final integrated lifecycle checks remain separate from controller/headless coverage. Follow the [input contract](../widget-input.md), [verification record](../widget-input-verification.md), [controller source](../../crates/harbor-widget/src/text_editing.rs) and [input integration tests](../../crates/harbor-widget/tests/text_input.rs).

## Route focus and commands

| Need | Available API | Boundary / evidence |
| --- | --- | --- |
| Stable focus request | `FocusHandle`, `Focus`; `Runtime::request_focus` / `WinitWindowHost::request_focus` | Retain the handle, attach it to the intended control, ask the host/runtime to focus it; [source](../../crates/harbor-widget/src/widgets/focus.rs) |
| Traversal or in-runtime modal routing | `FocusScope`, `.modal(true)` | Tab/Shift+Tab and modal routing within one runtime, not a cross-window dialog product; [focus/action tests](../../crates/harbor-widget/tests/interaction_focus_actions_theme.rs) |
| Typed shortcut routing | `Shortcuts<A>`, `KeyChord`, `Actions<A>`, `ActionOutcome` | Nearest compatible ancestor action handler can consume or pass through; separate from Store queues; [source](../../crates/harbor-widget/src/widgets/shortcuts.rs) |
| Low-level event behavior | `input::event::UiEvent`, `input::event_ctx::EventCtx` | Capture/target/bubble routing, focus/capture/paint commands; [routing tests](../../crates/harbor-widget/tests/input_routing.rs) |

Focus and labels do not implement Windows UIA, a semantic accessibility tree or screen-reader support. Application shortcut precedence and cross-window policy remain application-owned.

## Style a surface

| Need | Available API | Boundary / evidence |
| --- | --- | --- |
| Inherited control appearance | `ThemeProvider`, `Theme`, `ControlStyle`, `ControlColors`, `ControlVisualState` | Theme inheritance and per-control style, not a locale/direction/environment framework; [theme tests](../../crates/harbor-widget/tests/interaction_focus_actions_theme.rs) |
| Fill, border, corners, outer shadow, child clipping | `DecoratedBox`, `BoxDecoration`, `Border`, `BorderRadius`, `BoxShadow`, `ClipBehavior` | Layout-neutral decoration; shadow → background → child → border; [decoration tests](../../crates/harbor-widget/tests/decorated_box.rs) |
| Reusable gradient fill or stroke | `Paint`, `LinearGradient`, `GradientStop` | Normalized box coordinates; 2–8 strictly increasing stops; premultiplied linear-RGBA interpolation; [paint source/tests](../../crates/harbor-widget/src/paint.rs) |
| Soften already-painted content locally | `BackdropFilter` | Real local backdrop sampling, including external draws, with sharp child content; not whole-window Acrylic; [source/tests](../../crates/harbor-widget/src/widgets/backdrop_filter.rs) |
| Bend only the backdrop near edges/corners | `EdgeRefraction` via `.edge_refraction(...)` | Bounded optional visual lens approximation; stable center/foreground, not physical optics or arbitrary filters; [frame/GPU tests](../../crates/harbor-widget/tests/quad_renderer.rs) |

These are delivered primitives, **not** an absent feature list. `BackdropFilter::new` validates finite nonnegative sigma and caps it at 64 logical pixels. `EdgeRefraction::new(width, displacement)` validates nonnegative finite values and caps width/displacement at 64/32. Zero blur still permits enabled edge refraction. Render filter scenes through `Runtime::encode_frame`; merely drawing ordinary scene quads is not the full filter path.

A consumer owns material tuning, translucent button styles, contrast and geometry. The library does not provide a comprehensive `Glass` component or search-panel preset. See [glass recipe](recipes.md), [requirements 0029](../../.grimoire/spec/0029-search-panel-glass-appearance.md) and [testing](testing.md) for visual acceptance boundaries.

## Escape ordinary painting or layout

| Need | Available API | Boundary / evidence |
| --- | --- | --- |
| Root overlay | `Portal` | Escapes ordinary parent layout slot, painting order and clipping; retains logical ancestry/lifetime; no automatic focus, dismissal or modal scope; [source](../../crates/harbor-widget/src/widgets/portal.rs), [boundary tests](../../crates/harbor-widget/tests/component_boundaries.rs) |
| External renderer/input integration | `widgets::custom_paint::CustomPaint` | External draw ID, draw/schedule/IME/input/unmount callbacks; application owns the embedded renderer; [external input tests](../../crates/harbor-widget/tests/external_input.rs) |
| Ordinary native window | `winit::WinitWindowHostBuilder`, `WinitWindowHost`, `SharedGpu` | Feature-gated per-window host; application owns event loop/business/multi-window policy; [native smoke source](../../crates/harbor-widget/tests/native_host_smoke.rs) |

An overlay is not an OS window. Harbor's paste-confirmation policy uses a separate OS window; Portal does not replace that security/application decision. For embedding a terminal, see [TERMINAL](../terminal/README.md) and [runtime architecture](../architecture/widget-runtime.md).

## Boundaries to plan around

No generic modal-dialog/menu/popover service, UIA bridge, virtual collections, wrapping/grid/positioned layout suite, arbitrary SVG/images/paths/filters, full rich text or concurrent rendering is promised here. Some application-specific compositions may exist; that does not create a reusable widget API. See [remaining toolkit work](#remaining-toolkit-work) and [targets](../targets.md) for future scope and platform/support boundaries.

## Remaining Toolkit Work

This is the remaining desktop capability scope, not another inventory of delivered APIs or a release schedule. [Roadmap](../roadmap.md) owns product sequencing; [N01–N17](../next-stage-plan.md) owns product requirements. Reuse the foundations above rather than restarting P0. Source/automated coverage does not close [widget acceptance](testing.md#widget-acceptance-matrix).

### Desktop scope and design rules

The target is a reusable Windows-first desktop toolkit: mouse, keyboard, wheel/trackpad, focus, shortcuts, context menus/tooltips, resizable layouts, forms, shell/data controls, clipboard/IME, accessibility, high DPI, cursors and drag/drop. Large data views must stay efficient and quiet runtimes must not continuously redraw.

Mobile navigation/app bars/drawers/bottom sheets, phone safe-area/orientation/soft-keyboard helpers, mobile pickers, swipe/pull-to-refresh/long-press-first interactions, broad multi-touch gestures and Cupertino controls are not initial catalog goals. A later desktop need may justify a shared mechanism, not upstream API/lifecycle/rendering compatibility. See [comparison boundaries](../targets.md#widget-compared-with-react-and-flutter).

- Build mechanisms before dependent controls; prefer composition to aliases or a monolithic container/shell widget.
- Keep behavior wrappers layout-neutral unless documented otherwise. Define keyboard activation, focus, hover, disabled state, cursor and semantics as part of correctness.
- Prefer controlled values plus change callbacks; use reusable controllers for externally addressed editing, selection or scrolling. Form validation/submission coordinates controlled fields, not a second state system.
- Consume semantic theme tokens rather than product palettes. Preserve application ownership of routing, notification policy, terminal/session state and material presets.
- Virtualize unbounded collections from the start; do not materialize all rows and merely skip painting them. Prefer a small reusable viewport protocol over copying a full sliver hierarchy.
- Keep clipboard, IME, accessibility, drag/drop and window operations at the platform-neutral host-effect boundary; do not introduce winit types into core widgets. In-window menus and native host menus are separate contracts.

### Dependency tiers and remaining capabilities

P0–P3 retain the former plan's dependency tiers, not deadlines or claims that an entire tier is absent. Product work may select only needed slices. True dirty-subtree rebuilding remains follow-up work, not a reason to rebuild delivered foundations.

| Tier | Remaining area | Scope / dependency |
| --- | --- | --- |
| P0 | Sizing and responsive layout | Aspect ratio, fractional sizing and positioned stack children as needed; constraint-aware builder and inherited window metrics. Committed layout observation is not build-during-layout. Extend intrinsic/baseline protocols only for concrete needs. |
| P0–P1 | Pointer, focus and commands | General pointer listener, desktop double-click/drag behavior, richer traversal/roving focus and reusable default/cancel actions; reuse shared interaction state. |
| P0–P2 | Semantics and environment | Define painting-independent role/name/value/state/action nodes and selected/checked/invalid control contracts first; Windows UIA bridge follows in P2. Extend theme inheritance with window metrics, text direction and locale/string lookup. |
| P1 | Scrolling and overlays | Generic scrollbar and ensure-visible/scroll-policy extensions as needed. Extend Portal with anchoring, edge flip/clamp, modal barrier/routing, focus trap/restore and Escape/default actions before reusable tooltip/popover/dialog composition. |
| P1 | Controls | Button variants/content slots and semantics; checkbox, radio group, toggle/switch, slider, segmented controls and determinate progress. Reuse existing interaction/theme mechanisms. |
| P1–P2 | Text and forms | Proportional text runs, wrapping, alignment, max lines and ellipsis, then spans/selectable text. Close delivered single-line editor native acceptance; add validation/form composition or broader editing only for a concrete need. |
| P1–P2 | Menus and choice | Menu model, in-window menu bar, context/popup menus and combo box, with integrated shortcuts and roving focus. |
| P1–P2 | Painting and resources | Explicit opacity/transform and repaint-boundary/layer decisions; general raster/image resource contract with fit/alignment and placeholder/error states, plus an SVG-or-path strategy. Built-in icons and local filters are not that general pipeline. |
| P2 | Flow, grid and data | Wrap with spacing/run alignment; shared grid track solver before finite grid/table. VirtualList first, then a reusable viewport adapter for virtual grid/table/tree with selection, resizing, sorting, keyboard navigation and ensure-visible. |
| P2 | Desktop shell and feedback | Toolbar, breadcrumbs, split panes and status surfaces; generalize product tabs/sidebar only where reuse requires it. Surface/list-row and badge/status recipes, indeterminate progress and additional feedback policies remain open. |
| P2 | Animation and examples | Clock/ticker, controller, curves and reduced-motion policy before a small transition set. Catalog examples: settings panel, command palette, data explorer and paste-confirmation dialog. |
| P3 | Proven extensions only | Require concrete product needs and profiling evidence for native drag/drop, file dialogs/window commands; multiline/rich or complex shaping/bidi editing; heterogeneous-span virtual grids, advanced docking/detachable panes; path/SVG/filter/richer compositing; genuine multi-page routing; gestures beyond desktop click/double-click/drag/wheel. |

### Tier exit gates

These remain acceptance outcomes, not statements of current completion. Apply the [widget matrix](testing.md#widget-acceptance-matrix) and shared [validation policy](../validation.md).

| Tier | Exit evidence |
| --- | --- |
| P0 | Resizable settings-style panel without custom layout code: deterministic Flex, stable keyed state after reorder, complete keyboard traversal, themed visuals and a queryable semantic tree. |
| P1 | Dialog with themed labels, editable fields, toggles, scrolling, validation, default/cancel buttons, tooltip and popup choice; mouse/keyboard operation, correct focus, IME composition and no idle redraw. |
| P2 | Keyboard-first desktop shell hosting large data without building every row, meaningful Windows accessibility, resize/DPI resilience and measured bounded per-frame work under documented workloads. |
| P3 | No blanket exit gate: justify and verify each extension against its concrete requirement. |

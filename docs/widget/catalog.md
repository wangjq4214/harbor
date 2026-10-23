# Widget catalog

[WIDGET home](README.md) · [Learn](learn.md) · [Recipes](recipes.md)

Browse by the job you need to do. The tables describe available public APIs and their constraints.

## Lay out content

| Need | Available building blocks | Constraints |
| --- | --- | --- |
| Horizontal or vertical composition | `Row`, `Column`, `Flex`, `MainAxisAlignment`, `Alignment`, `.gap(...)` | Shared constrained flex engine, not CSS; [layout contract](../flex-layout.md) |
| Allocate flexible space | `Expanded`, `Flexible`, `FlexFit`, `Spacer` | Flex-parent relationships and bounded/unbounded constraints matter |
| Impose size, insets or alignment | `SizedBox`, `ConstrainedBox`, `Padding`, `Align` | Logical pixels; `Align` can position overlay content without making its empty area hit-testable |
| Layer ordinary children | `Stack` | Shared parent allocation, not a generic positioned/anchored popover engine |
| Scroll a single content subtree | `ScrollView`, `ScrollController`, `ScrollMetrics` | Vertical clipped viewport, overflow-only draggable scrollbar, track paging and controller-visible geometry; not virtualized lists/trees/tables |
| Observe committed allocation | `LayoutObserver`, `LayoutChangedCallback` | Geometry notification, not application resize policy |
| Add a rule | `Separator` | Layout/presentation primitive |

`ScrollView` replaces the former `ScrollArea` name and module (`widgets::scroll_view`); no old-name alias is retained. Wheel/key movement and scrollbar gestures preserve focus. `ScrollController::reveal_range(start, end)` minimally reveals a content-local vertical range using committed metrics; ordinary focus traversal also reveals offscreen controls. `ConstrainedBox::max_height_with` can cap a viewport or its surrounding layout using current `TextMetrics`, without hard-coding font sizes.

`SizedBox`, `Padding`, `Align` and `Stack` are imported through `harbor_widget::widgets::<module>::Type`, rather than all being crate-root exports. Check [public exports](../../crates/harbor-widget/src/lib.rs) instead of guessing import paths from type names.

## Display and activate

| Need | Available control | Constraints |
| --- | --- | --- |
| Single-line text | `widgets::text_label::TextLabel`, `.color(...)`, `.ellipsis()` | Monospace label, not rich text or a general shaping/layout engine |
| Built-in vector artwork | `Icon`, `LucideIcon` | Built-in Lucide inventory, not arbitrary SVG loading |
| Labelled activation | `Button` | `.on_click`, `.disabled`, `.selected`; uses the shared interaction state machine |
| Icon activation | `IconButton` | Icon plus label and control styling; the label is not proof of a native accessibility bridge |
| Compose custom visuals with activation | `InteractiveRegion`, `InteractionState`, `ControlStyle` | Hover/press/focus/disabled/selected state and `.content(...)`/`.child(...)`; not a gesture-recognition suite |
| Pointer-specific response | `MouseRegion` | Lower-level pointer hooks/cursor behavior; use an existing control when activation is the goal |
| Transient dismissible message | `Notification` | A reusable message composition, not a global notification center, timer service or OS toast API |

Button activation callbacks require `Send + Sync`; use a `Dispatcher` for application intent rather than capturing a UI-thread signal. See [Learn step 3](learn.md).

## Edit a single line

| Layer | Available API | Ownership |
| --- | --- | --- |
| Coherent editing state | `TextEditingController`, `TextEditingValue`, `TextSelection` | Text, selection/caret, composition, revision, history and acceptance predicate |
| Decoration-free editor | `EditableText` | Editing input, focus, caret/selection geometry, horizontal scrolling and clipping |
| Decorated field | `TextField` | Composes that same editor with padding and `BoxDecoration`; placeholder, enabled and read-only modes |

Use `TextField::new(controller.clone())`, not a parallel application string editor. It is already focusable: assign `.focus_handle(handle)` directly instead of adding another focus stop.

Current behavior includes grapheme-safe navigation/replacement/deletion, selection drag, local Windows editing chords, undo/redo and tagged clipboard completion. Public positions are **UTF-8 byte offsets normalized to extended grapheme boundaries**, not character counts. `on_changed` reports user committed-text changes only; programmatic replacement, selection movement and preedit do not fire it. `on_submitted` receives Enter modifiers; `on_escape` leaves cancellation policy to the consumer.

This is single-line plain text, not multiline, password or rich-text editing. See the [input contract](../widget-input.md).

## Route focus and commands

| Need | Available API | Constraints |
| --- | --- | --- |
| Stable focus request | `FocusHandle`, `Focus`; `Runtime::request_focus` / `WinitWindowHost::request_focus` | Retain the handle, attach it to the intended control, ask the host/runtime to focus it |
| Traversal or in-runtime modal routing | `FocusScope`, `.modal(true)` | Tab/Shift+Tab and modal routing within one runtime, not a cross-window dialog product |
| Typed shortcut routing | `Shortcuts<A>`, `KeyChord`, `Actions<A>`, `ActionOutcome` | Nearest compatible ancestor action handler can consume or pass through; separate from Store queues |
| Low-level event behavior | `input::event::UiEvent`, `input::event_ctx::EventCtx` | Capture/target/bubble routing, focus/capture/paint commands |

Focus and labels do not implement Windows UIA, a semantic accessibility tree or screen-reader support. Application shortcut precedence and cross-window policy remain application-owned.

## Style a surface

| Need | Available API | Constraints |
| --- | --- | --- |
| Inherited control appearance | `ThemeProvider`, `Theme`, `ControlStyle`, `ControlColors`, `ControlVisualState` | Theme inheritance and per-control style, not a locale/direction/environment framework |
| Fill, border, corners, outer shadow, child clipping | `DecoratedBox`, `BoxDecoration`, `Border`, `BorderRadius`, `BoxShadow`, `ClipBehavior` | Layout-neutral decoration; shadow → background → child → border |
| Reusable gradient fill or stroke | `Paint`, `LinearGradient`, `GradientStop` | Normalized box coordinates; 2–8 strictly increasing stops; premultiplied linear-RGBA interpolation |
| Soften already-painted content locally | `BackdropFilter` | Real local backdrop sampling, including external draws, with sharp child content; not whole-window Acrylic |
| Bend only the backdrop near edges/corners | `EdgeRefraction` via `.edge_refraction(...)` | Bounded optional visual lens approximation; stable center/foreground, not physical optics or arbitrary filters |

`BackdropFilter::new` validates finite nonnegative sigma and caps it at 64 logical pixels. `EdgeRefraction::new(width, displacement)` validates nonnegative finite values and caps width/displacement at 64/32. Zero blur still permits enabled edge refraction. Render filter scenes through `Runtime::encode_frame`; merely drawing ordinary scene quads is not the full filter path.

A consumer owns material tuning, translucent button styles, contrast and geometry. The library does not provide a comprehensive `Glass` component or search-panel preset. See the [glass recipe](recipes.md).

## Escape ordinary painting or layout

| Need | Available API | Constraints |
| --- | --- | --- |
| Root overlay | `Portal` | Escapes ordinary parent layout slot, painting order and clipping; retains logical ancestry/lifetime; no automatic focus, dismissal or modal scope; [source](../../crates/harbor-widget/src/widgets/portal.rs) |
| External renderer/input integration | `widgets::custom_paint::CustomPaint` | External draw ID, draw/schedule/IME/input/unmount callbacks; application owns the embedded renderer |
| Ordinary native window | `winit::WinitWindowHostBuilder`, `WinitWindowHost`, `SharedGpu` | Feature-gated per-window host; application owns event loop/business/multi-window policy |

An overlay is not an OS window. For embedding a terminal, see [TERMINAL](../terminal/README.md).

## Boundaries to plan around

There is no generic modal-dialog/menu/popover service, UIA bridge, virtual collections, wrapping/grid/positioned layout suite, arbitrary SVG/images/paths/filters, full rich text or concurrent rendering API.

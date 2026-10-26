# Widget catalog

[WIDGET home](README.md) · [Learn](learn.md) · [Recipes](recipes.md)

Browse by the job you need to do. The tables describe available public APIs and their constraints.

## Lay out content

| Need | Available building blocks | Constraints |
| --- | --- | --- |
| Horizontal or vertical composition | `Row`, `Column`, `Flex`, `FlexOptions`, `MainAxisAlignment`, `CrossAxisAlignment` | Shared constrained flex engine; fallible options/gaps; [layout contract](../flex-layout.md) |
| Allocate flexible space | `Expanded`, `Flexible`, `FlexFit`, `Spacer` | Flex-parent relationships and bounded/unbounded constraints matter |
| Impose size, insets or alignment | `SizedBox`, `ConstrainedBox`, `Padding`, `Align` | Logical pixels; `Align` can position overlay content without making its empty area hit-testable |
| Layer ordinary children | `Stack` | Shared parent allocation, not a generic positioned/anchored popover engine |
| Scroll a single content subtree | `ScrollView`, `ScrollController`, `ScrollMetrics` | Vertical clipped viewport, overflow-only draggable scrollbar, track paging and controller-visible geometry; not virtualized lists/trees/tables |
| Observe committed allocation | `LayoutObserver`, `LayoutChangedCallback` | Geometry notification, not application resize policy |
| Add a rule | `Separator` | Layout/presentation primitive |

`ScrollController::requested_offset()` reports the normalized request; `controller.metrics().offset()` reports the latest successful layout position. A request can differ from the committed offset until successful layout clamps and commits it.

`ScrollView` replaces the former `ScrollArea` name and module (`widgets::scroll_view`); no old-name alias is retained. Wheel/key movement and scrollbar gestures preserve focus. `ScrollController::reveal_range(start, end)` minimally reveals a content-local vertical range using committed metrics; ordinary focus traversal also reveals offscreen controls. `ConstrainedBox::max_height_with` can cap a viewport or its surrounding layout using current `TextMetrics`, without hard-coding font sizes.

`SizedBox`, `Padding`, `Align`, `Insets` and `Alignment2D` are available at the crate root; `Stack` and `TextLabel` are also root exports. See [explicit layout and fallible nodes](layout.md) for validation, slot replacement and wrapper order.

## Display and activate

| Need | Available control | Constraints |
| --- | --- | --- |
| Single-line text | `TextLabel`, `.foreground(...)`, `.ellipsis()` | Renderer-advance label, not rich text or a general shaping/layout engine |
| Built-in vector artwork | `Icon`, `LucideIcon` | Built-in Lucide inventory, not arbitrary SVG loading |
| Composable activation | `Button::new()`, `Button::text(text)`, `Button::icon(icon, text)` | Single replaceable child/content slot; `.on_click`, `.disabled`, `.selected`, `.focus_handle`, `.style` |
| Icon activation | `IconButton::new(icon, label)` | Icon-only; label is non-painted metadata, not proof of a native accessibility bridge |
| Compose custom visuals with activation | `InteractiveRegion`, `InteractionState`, `ControlStyle` | Hover/press/focus/disabled/selected state and `.child_with(...)`/`.child(...)`; not a gesture-recognition suite |
| Pointer-specific response | `MouseRegion` | Lower-level pointer hooks/cursor behavior; use an existing control when activation is the goal |
| Transient dismissible message | `NotificationCard` | A reusable message composition, not a global notification center, timer service or OS toast API |

Button/IconButton `on_click`, IconButton `foreground_with`, MouseRegion `on_enter`/`on_exit` and NotificationCard dismissal accept UI-local `Fn(...) + 'static` captures, stored with `Rc`. Use direct Signals for local UI state and Dispatcher for business intent; see [Learn](learn.md). Button and IconButton accept `.focus_handle(handle)` on their existing interactive region, without another focus wrapper.

### Button content and appearance

The old text-taking constructor is removed; use `Button::text(text)`. `Button::new()`
is empty and accepts a component or existing View through `.child(...)`.
All four content forms are ordinary deferred descriptions:

```rust
use harbor_widget::{Button, Column, LucideIcon, view};
use harbor_widget::Component;
use harbor_widget::TextLabel;
use harbor_widget::scene::primitive::Color;

let controls = view! { Column::new() => {
    Button::text("Save");
    Button::icon(LucideIcon::Plus, "New");
    Button::new() => { TextLabel::new("Custom").foreground(Color::GREEN); }
    Button::new().child_with(|cx, state| {
        TextLabel::new(if state.pressed { "Release" } else { "Press" })
            .foreground(Color::BLUE).expect("finite foreground").build(cx)
    });
}};
```

Conveniences, `.child(...)`, `.child_with(...)` and macro blocks select **one slot**:
later choices replace earlier choices, repeated children do not append, and an
empty macro block clears even a convenience or builder. Multiple incoming
children fail statically where provable, otherwise at construction (handwritten
`ChildConstructionError`; ordinary `view!` panic). Content builders receive
`InteractionState` on their own child Fiber with independent hooks/subscriptions.

Button defaults to `theme.button`; IconButton defaults to `theme.icon_button`.
Both share surface and interaction mechanics; `.style(ControlStyle)` overrides
the local role. Built-in Button text/icon foreground follows the same style/state
as its surface. Custom children retain their own explicit colors; there is no
automatic foreground inheritance. State-driven custom content can select its
own state-dependent presentation.

Built-in Button content is centered, single-line and measured by TextLabel with
renderer glyph advances (its ordinary fallback without font resources). Narrow
space uses TextLabel's grapheme-safe ellipsis, reserving icon and theme spacing
first; wider layouts recover the retained full text. Parent constraints remain
authoritative even below padding/icon size. Custom content chooses its own
wrapping/truncation. IconButton stays icon-only: its label is metadata, while
Button's `icon(icon, text)` paints visible text. IconButton's `icon_size` and
`foreground_with` overrides retain their scope and interaction timing. Query the non-painted label with `get_label()`. Existing styled InteractiveRegion
compositions need no new surface wrapper.

### Appearance validation

Direct appearance inputs return `Result`: `Icon::size`, `IconButton::icon_size`,
`TextField::padding`, `Separator::thickness`, ordinary `foreground` setters,
`SizedBox/Align/Padding/Row/Column/Flex/Stack::background`, `Separator::color`,
control `style` setters, and
`ThemeProvider::new`. Dimensions and theme spacing must be finite and nonnegative
(zero is legal). Solid colors require finite RGBA components, not [0,1] bounds.
`ControlStyle::validate()` checks every state and focus-ring color;
`Theme::validate()` also checks both roles and spacing. Public configuration
structs are raw inputs, not validation tokens. Errors name the field and rule.

`ThemeProvider::from_signal` validates the whole theme on every consumption,
including the first one. `IconButton::foreground_with` validates every consumed
result. Invalid dynamic values panic with the field; there is no fallback or
post-panic Runtime recovery promise. Providers retain their original UI-local
capture and evaluation timing.

Handle direct errors before attaching children or callbacks. Ordinary `view!`
handles only the **final node** Result, panicking with error and source location;
it does not unwrap intermediate chains:

```rust
use harbor_widget::{Icon, LucideIcon, SizedBox, Theme, ThemeProvider, view};
use harbor_widget::scene::primitive::Color;

// Recoverable direct calls (inside a Result-returning function):
let icon = Icon::new(LucideIcon::Plus).size(18.0)?;
let icon = icon.foreground(Color::WHITE)?;
let themed = ThemeProvider::new(Theme::default())?.child(icon);

// Final fallible nodes work without an intermediate unwrap:
let box_view = view! { SizedBox::width(40.0).unwrap().background(Color::BLACK); };
# Ok::<(), Box<dyn std::error::Error>>(())
```

A legal finite scalar is not a guarantee that its derived layout combinations
fit in f32: actual overflow remains a layout error, checked before parent clamps.
Legal unbounded parent maxima and existing layout scalar rules are unchanged.

## Edit a single line

| Layer | Available API | Ownership |
| --- | --- | --- |
| Coherent editing state | `TextEditingController`, `TextEditingValue`, `TextSelection` | Text, selection/caret, composition, revision, history and acceptance predicate |
| Decoration-free editor | `EditableText` | Editing input, focus, caret/selection geometry, horizontal scrolling and clipping |
| Decorated field | `TextField` | Composes that same editor with padding and `BoxDecoration`; placeholder, disabled and read-only modes |

TextField and EditableText are enabled by default. `.disabled(true)` disables and `.disabled(false)` enables; the old `.enabled(value)` builder is removed (migrate to `.disabled(!value)`). Read-only remains separate and allows selection/copy.
Use `TextField::new(controller.clone())`, not a parallel application string editor. It is already focusable: assign `.focus_handle(handle)` directly instead of adding another focus stop.

Current behavior includes grapheme-safe navigation/replacement/deletion, selection drag, local Windows editing chords, undo/redo and tagged clipboard completion. Public positions are **UTF-8 byte offsets normalized to extended grapheme boundaries**, not character counts. `on_change` reports user committed-text changes only; programmatic replacement, selection movement and preedit do not fire it. `on_submit` receives Enter modifiers; `on_escape` leaves cancellation policy to the consumer.

This is single-line plain text, not multiline, password or rich-text editing. See the [input contract](../widget-input.md).

## Route focus and commands

| Need | Available API | Constraints |
| --- | --- | --- |
| Stable focus request | `FocusHandle`, `Focus`; `Runtime::request_focus` / `WinitWindowHost::request_focus` | Retain the handle, attach it to the intended control, ask the host/runtime to focus it |
| Traversal or in-runtime modal routing | `FocusScope`, `.modal(true)` | Tab/Shift+Tab and modal routing within one runtime, not a cross-window dialog product |
| Typed shortcut routing | `ShortcutMap<A>`, `KeyChord`, `ActionHandler<A>`, `ActionOutcome` | Nearest compatible ancestor action handler can consume or pass through; separate from Store queues |
| Low-level event behavior | `input::event::UiEvent`, `input::event_ctx::EventCtx` | Capture/target/bubble routing, focus/capture/paint commands |

Construct chords with `KeyChord::new(key, modifiers)` and inspect them with
`key()` / `modifiers()`; fields are private. ASCII character keys normalize to
lowercase without changing modifiers or merging ordinary/keypad keys. Non-ASCII
characters are unchanged. Within one `ShortcutMap` instance, the first matching
binding wins, including duplicates that differ only in ASCII case. This does not
relax Harbor's application-level duplicate/conflict validation.

`View::new` and Signal `subscribe` / `unsubscribe` are internal, not public
construction or tracking APIs. Compose with `Component` / `IntoChildView`, widget
child setters or `view!`, and use `.keyed(...)` for explicit identity. Extend
primitive leaves with `View::leaf`. Track external state in a mounted build with
`BuildCx::track` or `Store::watch`; use `BuildCx::use_state` for hook state. Reads
alone do not subscribe, and state publication does not automatically wake a host.
Read-only inspection through `Runtime::arena` remains available.

`Focus::new()`, `ShortcutMap::new()`, and `ActionHandler::new(handler)` start without children; `.child(...)` replaces their single slot. Macro blocks replace it too, including clearing with `{}`. Focus uses `.focus_handle(handle)`, `.tab_order(order)`, and `.focusable(bool)`: focusability is not a blanket disable of subtree input. Disabled controls and read-only editing keep their separate policies.

`LayoutObserver::new(callback)` requires a callback; `from_shared_callback(handle)` retains the actual shared callback handle. Notifications still run after successful layout, paint and dirty settlement, not during measurement.

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
| External renderer/input integration | `ExternalSurface` | `on_draw`, `schedule_with`, `ime_with`, `on_input`, `on_unmount`; ExternalDrawId links the providers; application owns the embedded renderer |
| Ordinary native window | `winit::WinitWindowHostBuilder`, `WinitWindowHost`, `SharedGpu` | Feature-gated per-window host; application owns event loop/business/multi-window policy |

An overlay is not an OS window. For embedding a terminal, see [TERMINAL](../terminal/README.md).

## Boundaries to plan around

There is no generic modal-dialog/menu/popover service, UIA bridge, virtual collections, wrapping/grid/positioned layout suite, arbitrary SVG/images/paths/filters, full rich text or concurrent rendering API.

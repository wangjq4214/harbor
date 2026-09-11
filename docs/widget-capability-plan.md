# Widget Capability Plan

> Status: Proposed
>
> Scope: Desktop-first capability planning for `crates/harbor-widget`. Project scheduling and release priority remain owned by [`roadmap.md`](roadmap.md); the implemented runtime contract remains documented in [`architecture/widget-runtime.md`](architecture/widget-runtime.md).

## Purpose

Harbor should borrow Flutter's separation of layout primitives, interaction behaviors, controls, and application-level composition without copying Flutter's API one widget at a time. The target is a coherent Windows-first desktop toolkit for Harbor's own windows, not a general mobile UI framework.

This plan therefore:

- compares Harbor's current runtime with the desktop-relevant parts of Flutter's widget catalog;
- identifies runtime mechanisms that must precede higher-level controls;
- groups related Flutter widgets into smaller Harbor primitives where composition is sufficient;
- excludes mobile-only navigation, touch-first gestures, and platform-specific Cupertino controls;
- defines dependency order and evidence gates, but not product release dates.

## Current Baseline

The current runtime already provides a useful retained-mode foundation:

| Area | Available now | Important limitation |
| --- | --- | --- |
| Declaration and state | `Component`, immutable `View`, `Fiber`, `Signal`, positional hooks | Keyed sibling reordering and true dirty-subtree rebuilds are incomplete |
| Layout | `BoxConstraints`, `SizedBox`, `Padding`, `Align`, `Row`, `Column`, `Stack`, `FocusScope` | No flexible allocation, main-axis distribution, positioned children, wrapping, grid, or intrinsic/baseline protocol |
| Paint | Quads, rounded fills, borders, outer shadows, text runs, descendant clips, `CustomPaint` | No image, icon, path, transform, opacity layer, or general compositing primitives |
| Text | `TextLabel`, shared `harbor-text` glyph metrics/cache | Single-line monospace measurement; no wrapping, selection, editing, rich spans, shaping, bidi, or ellipsis |
| Input | Pointer, wheel, keyboard, focus, capture/target/bubble routing, pointer capture, Tab traversal, IME commit delivery | No reusable listener/mouse-region/shortcut/action abstractions, focus policy, drag-and-drop, or text-editing model |
| Controls | Focusable `Button`; Harbor-specific `PreviewPane` | Styling is hard-coded; no disabled state, control families, forms, or shared interaction-state model |
| Host effects | Redraw, scheduling, cursor, IME, clipboard; optional winit adapter | Core mechanisms exist, but reusable widgets do not yet expose most of them |
| Quality | Unit and integration coverage for layout, routing, rendering, effects, winit, and external paint | No widget catalog examples, accessibility contract, or visual-state test matrix |

`PreviewPane` and `CustomPaint` are product integration widgets rather than general catalog primitives. They should remain supported, but they should not substitute for generic scrolling, text, or input widgets.

## Desktop Scope

### In scope

- Mouse, keyboard, wheel/trackpad, focus rings, shortcuts, context menus, tooltips, and resizable layouts.
- Menu bars, toolbars, tabs, side navigation, split panes, dialogs, popovers, forms, lists, tables, and trees.
- Text input with selection, clipboard, IME preedit/commit, and candidate positioning.
- Windows accessibility integration, high-DPI behavior, desktop cursors, and drag-and-drop.
- Efficient large data views and idle runtimes that do not continuously redraw.

### Explicitly out of scope

The initial catalog does not include:

- Cupertino widgets or iOS-style adaptive controls;
- `BottomNavigationBar`, `NavigationBar`, `BottomSheet`, `FloatingActionButton`, mobile drawers, or mobile app bars as first-class controls;
- `SafeArea` for phone notches, orientation-driven phone layouts, or soft-keyboard avoidance helpers;
- pull-to-refresh, swipe-to-dismiss, page swiping, long-press-first interactions, or broad multi-touch gesture recognition;
- mobile pickers and mobile-only date/time presentation;
- a compatibility promise with Flutter names, constructors, lifecycle, or rendering behavior.

A desktop use case may later justify a shared underlying mechanism. For example, a modal surface can support a desktop dialog without introducing bottom sheets.

## Design Rules

1. **Mechanism before catalog breadth.** Add focus, semantics, overlays, scrolling, styling, and text-editing contracts before controls that depend on them.
2. **Composition over aliases.** Do not add Harbor types solely because Flutter has separate names for equivalent compositions.
3. **Desktop behavior is part of correctness.** Every interactive control must define keyboard activation, focus, hover, disabled state, cursor, and accessibility semantics.
4. **Behavior wrappers stay layout-neutral.** Focus, pointer, shortcut, semantics, and tooltip wrappers should normally own one child and preserve its layout unless explicitly documented otherwise.
5. **State is controlled where practical.** Value controls accept current value plus change callbacks; reusable controllers are introduced only for selection, scrolling, text editing, or other state that must be addressed externally.
6. **Theme tokens replace hard-coded visuals.** Controls consume semantic colors, typography, spacing, radii, and interaction-state tokens rather than embedding a product palette.
7. **Virtualize from the first unbounded collection.** A generic large list, table, or tree must not materialize all rows and paint only a subset afterward.
8. **Keep platform operations at the host boundary.** Widgets request clipboard, IME, cursor, accessibility, drag/drop, and window actions through platform-neutral effects.

## Flutter-to-Harbor Capability Map

The references in this table are conceptual. A row can map several Flutter widgets onto one Harbor mechanism.

| Capability | Flutter reference concepts | Harbor status | Planned Harbor surface | Tier |
| --- | --- | --- | --- | --- |
| Constraints and fixed sizing | `ConstrainedBox`, `UnconstrainedBox`, `LimitedBox`, `FractionallySizedBox`, `AspectRatio` | Partial (`SizedBox`, `BoxConstraints`) | `ConstrainedBox`, `AspectRatio`, fractional sizing; add unconstrained layout only for a demonstrated case | P0 |
| Responsive composition | `LayoutBuilder`, `MediaQuery` | Missing | Constraint-aware builder plus inherited window metrics; choose breakpoints by available space rather than device class | P0 |
| Linear layout | `Flex`, `Row`, `Column`, `Expanded`, `Flexible`, `Spacer` | Partial | One `Flex` engine with `Row`/`Column` facades, flex factors, fit, gaps, main/cross alignment, and overflow diagnostics | P0 |
| Overlay layout | `Stack`, `Positioned`, `Center`, `Align` | Partial | Positioned/aligned stack children; keep `Center` as convenience rather than a new engine | P0 |
| Flow layout | `Wrap` | Missing | `Wrap` with spacing, run spacing, and alignment | P1 |
| Repeated two-dimensional layout | `GridView`, `Table` | Missing | Shared grid track solver, then finite `Grid` and `Table`; virtualized variants remain separate | P2 |
| Paint wrappers | `DecoratedBox`, `ClipRect`, `ClipRRect`, `Opacity`, `Transform` | Partial | Explicit clip, opacity, transform, and repaint-boundary/layer decisions; preserve `DecoratedBox` | P1 |
| Images and icons | `Image`, `Icon`, `RawImage` | Missing | `Image` resource contract, fit/alignment, raster/SVG-or-path icon strategy, placeholder/error states | P2 |
| Text display | `Text`, `RichText`, `SelectableText` | Minimal | Proportional `Text`, wrap/max-lines/ellipsis/alignment, then spans and selectable text | P1-P2 |
| Pointer behavior | `Listener`, `MouseRegion`, `GestureDetector` | Runtime only | `PointerListener`, `MouseRegion`, cursor requests, click/double-click and desktop drag recognizers; no broad mobile gesture arena initially | P0-P1 |
| Focus and commands | `Focus`, `FocusScope`, `FocusTraversalGroup`, `Shortcuts`, `Actions` | Partial | Focus handle/policy/order, roving focus, `ShortcutMap`, typed commands/actions, default/cancel actions | P0 |
| Semantics | `Semantics`, `ExcludeSemantics`, `MergeSemantics` | Missing | Role/name/value/state/action tree independent of painting, followed by Windows UI Automation bridge | P0 contract, P2 bridge |
| Scrolling | `Scrollable`, `ScrollView`, `SingleChildScrollView`, `Scrollbar` | Product-specific only | `ScrollController`, viewport/extent protocol, wheel and keyboard policy, scrollbars, clipping, ensure-visible | P1 |
| Virtual collections | `ListView.builder`, slivers, `GridView.builder` | Missing | `VirtualList` first; reusable viewport adapter for virtual grid/table/tree without cloning Flutter's sliver API | P2 |
| Buttons | `TextButton`, `OutlinedButton`, `ElevatedButton`, `IconButton` | Minimal `Button` | Shared button behavior/state plus style variants and icon/content slots | P1 |
| Selection controls | `Checkbox`, `Radio`, `Switch`, `Slider`, segmented controls | Missing | Checkbox, radio group, toggle/switch, slider, segmented button with keyboard and semantic behavior | P1 |
| Text entry | `EditableText`, `TextField`, form fields | Missing | Editing model and controller, caret/selection, clipboard, undo/redo, IME preedit, then `TextField` and validation shell | P1-P2 |
| Menus and choice | `MenuBar`, `MenuAnchor`, `DropdownMenu`, popup menus | Missing | Menu model, menu bar, context menu, popup menu, combo box; integrated shortcuts and roving focus | P1-P2 |
| Overlay surfaces | `Overlay`, `Tooltip`, `Dialog`, popup routes | Missing | Overlay root/entry, anchored placement, modal barrier, focus restore, Escape/default actions; compose tooltip/popover/dialog | P1 |
| Navigation and shell | `Navigator`, `Scaffold`, tabs, navigation rail | Missing | Keep routing application-owned initially; provide tabs, toolbar, sidebar/navigation rail, breadcrumbs, status bar, and shell composition | P2 |
| Data presentation | `ListTile`, `Card`, `Divider`, `DataTable` | Mostly composable | `Separator`, optional `Surface`/`ListRow` recipes, then sortable/resizable/selectable table | P2 |
| Feedback | `ProgressIndicator`, badges, snack bars | Missing | Determinate/indeterminate progress, inline status/badge, overlay-backed toast/notification | P2 |
| Animation | `Animation`, `Tween`, implicit/explicit transitions | Scheduler only | Clock/ticker, animation controller, curves, reduced-motion policy, then a small transition set | P2 |
| Environment | `Theme`, `MediaQuery`, `Directionality`, localization | Missing | Inherited environment mechanism, theme tokens, scale/window metrics, text direction, locale/string lookup | P0-P2 |
| Desktop host integration | desktop window APIs beyond core Flutter widgets | Partial effects/winit | Drag/drop, file picker and window commands as host services; never embed winit types in core widgets | P3 |

## What Not to Copy from Flutter

Harbor should intentionally collapse or defer several Flutter concepts:

- `Container` should remain composition of sizing, padding, alignment, decoration, clip, and transform rather than becoming a large ambiguous widget.
- `Center` and `Spacer` can be thin conveniences over `Align` and flex, not independent layout implementations.
- `InkWell`, `GestureDetector`, and button subclasses should share one desktop interaction-state engine; ink effects are optional styling.
- Flutter's sliver family should become a smaller viewport/virtual-child protocol unless Harbor develops multiple scrolling layouts that prove the full abstraction necessary.
- `Scaffold` should be a documented desktop shell recipe, not a mobile page contract.
- In-window menu bars and native platform menus are different contracts. Harbor should build the former from widgets and expose the latter, if required, as a host service rather than assuming Flutter's platform-menu coverage.
- `Navigator` should not enter the core runtime until Harbor has multiple in-window routes; overlays and modal focus are the immediate reusable needs.
- `Form` should initially be validation and submission coordination around controlled fields, not a second state-management system.

## Delivery Tiers

The tiers are dependency order. Product work may select only the slices needed by the main window or paste-confirmation window.

### P0 — Contracts and Correct Layout

**Runtime and API**

- Define a documented public widget construction surface and a `widgets::prelude`; remove accidental dependence on crate-private `AnyView` details.
- Complete keyed sibling reordering before virtualized or dynamically reordered collections.
- Add an inherited environment mechanism for theme, window metrics, text direction, and later localization.
- Define semantic nodes and control interaction states (`disabled`, `hovered`, `pressed`, `focused`, `selected`, `checked`, `invalid`).

**Layout and interaction**

- Redesign the current single-pass, child-first layout contract so a parent can impose per-child constraints and remeasure when required; add lightweight parent layout data for flex factors and positioned children.
- Replace duplicate `Row`/`Column` logic with a flex engine supporting flex factors, loose/tight fit, gaps, main-axis distribution, cross-axis stretch, and overflow diagnostics.
- Add constrained sizing, aspect ratio, and positioned stack children.
- Add reusable pointer listener, mouse region/cursor, focus handle/order/policy, shortcuts, and actions.

**Exit gate**

A resizable settings-style panel can be built without custom layout code; it has deterministic flex behavior, stable keyed state after reorder, complete keyboard traversal, theme-driven visuals, and a queryable semantic tree.

### P1 — Desktop Interaction Core

**Scrolling and overlays**

- Introduce scroll metrics/controller, viewport clipping, wheel and keyboard scrolling, ensure-visible, and desktop scrollbar behavior.
- Add overlay entries, anchored placement with edge flipping/clamping, modal barriers, focus trapping/restoration, and Escape handling.
- Compose tooltip, popover, dialog, and notification surfaces from the overlay foundation.

**Text and controls**

- Upgrade text measurement and painting for proportional runs, wrapping, alignment, max lines, and ellipsis.
- Build the editing core: caret, selection, hit testing, clipboard, undo/redo, IME preedit/commit, candidate position, and horizontal scrolling.
- Refactor `Button` onto shared control behavior and theme styles; add icon button, checkbox, radio, toggle, slider, and determinate progress.
- Add menu primitives needed for menu bars, context menus, and simple combo boxes.

**Exit gate**

A desktop dialog can contain themed labels, editable fields, toggles, a scrollable region, validation, default/cancel buttons, a tooltip, and a popup choice menu. It is fully operable by mouse and keyboard, preserves focus correctly, supports IME composition, and idles without redraw.

### P2 — Application Shell and Data Widgets

- Add `Wrap`, finite grid/table layout, tabs, toolbar, sidebar/navigation rail, breadcrumbs, split panes, separators, and status surfaces.
- Add `VirtualList`, followed by virtualized table/tree adapters with row selection, resizing, sorting, keyboard navigation, and ensure-visible.
- Add selectable/rich text, image/icon support, indeterminate progress, and a small animation/transition toolkit with reduced-motion support.
- Connect the semantic tree to Windows UI Automation and add locale/string lookup plus text-direction propagation.
- Provide catalog examples covering a settings panel, command palette, data explorer, and paste-confirmation dialog.

**Exit gate**

A keyboard-first desktop shell can host large data sets without building all rows, expose meaningful Windows accessibility information, survive resize and DPI transitions, and demonstrate bounded per-frame work under documented workloads.

### P3 — Proven Desktop Extensions

Only add these from concrete product requirements and profiling evidence:

- native drag-and-drop, file dialogs, and window-management commands through host services;
- multi-line rich text editing, complex shaping, bidirectional editing, and advanced selection;
- virtual grids with heterogeneous spans, advanced docking, or detachable panes;
- path/SVG rendering, filters, and richer compositing;
- route/navigation infrastructure for genuine multi-page in-window applications;
- additional gestures beyond desktop click, double-click, drag, and wheel/trackpad scrolling.

## Cross-Cutting Acceptance Criteria

Every delivered interactive widget must provide proportionate evidence for:

- **Layout:** loose/tight constraints, minimum and maximum sizes, zero size, overflow, resize, and scale changes.
- **Interaction:** pointer capture/cancel, hover transitions, primary and secondary buttons where relevant, keyboard activation, focus traversal, disabled behavior, and shortcut conflicts.
- **Text input:** Unicode boundaries, selection, clipboard, undo/redo, IME preedit/commit, focus loss, and candidate positioning.
- **Overlays:** clipping, z-order, outside click, Escape, modal routing, focus restoration, and placement near each window edge.
- **Accessibility:** stable role/name/value/state/action output; keyboard-only completion of the representative workflow; Windows UIA smoke evidence once the bridge exists.
- **Rendering:** retained scene identity, transparent paint order, clipping, high DPI, GPU encode coverage where practical, and visual/runtime evidence for state matrices.
- **Performance:** no idle redraw, bounded invalidation, virtualization for unbounded collections, and measurements before introducing caches or spatial indexes.
- **Documentation:** one catalog example and public behavior notes per control family; local links and language policy pass [`validation.md`](validation.md).

Phase completion uses the standard quality gates in [`validation.md`](validation.md). Documentation-only updates require at least `python scripts/check_docs.py`.

## Recommended First Vertical Slice

The first implementation slice should not be another isolated visual control. Build the smallest dependency chain that proves desktop composition:

1. theme/environment and shared control-state contract;
2. corrected flex with `Expanded`/`Flexible` behavior and gaps;
3. pointer listener, mouse region/cursor, focus policy, shortcuts, and actions;
4. semantic tree contract;
5. refactored themed button with disabled and focus-visible states;
6. overlay root plus a keyboard-accessible dialog and tooltip;
7. generic scroll viewport plus scrollbar.

That slice removes the largest architectural blockers and supports Harbor's existing main and confirmation windows without committing to mobile abstractions or a broad general-purpose toolkit.

## References

- [Flutter widget catalog](https://docs.flutter.dev/ui/widgets)
- [Flutter Material widgets](https://docs.flutter.dev/ui/widgets/material)
- [Flutter widgets library](https://api.flutter.dev/flutter/widgets/widgets-library.html)
- [Flutter layout widgets](https://docs.flutter.dev/ui/widgets/layout)
- [Flutter input widgets](https://docs.flutter.dev/ui/widgets/input)
- [Flutter interaction widgets](https://docs.flutter.dev/ui/widgets/interaction)
- [Flutter scrolling widgets](https://docs.flutter.dev/ui/widgets/scrolling)
- [Flutter text widgets](https://docs.flutter.dev/ui/widgets/text)
- [Flutter accessibility documentation](https://docs.flutter.dev/ui/accessibility-and-internationalization/accessibility)
- [Flutter desktop support](https://docs.flutter.dev/platform-integration/desktop)
- [Flutter internationalization documentation](https://docs.flutter.dev/ui/internationalization)
- [Flutter adaptive and desktop input](https://docs.flutter.dev/ui/adaptive-responsive/input)
- [Flutter `MenuBar`](https://api.flutter.dev/flutter/material/MenuBar-class.html) and [`PlatformMenuBar`](https://api.flutter.dev/flutter/widgets/PlatformMenuBar-class.html)
- [Flutter `DataTable`](https://api.flutter.dev/flutter/material/DataTable-class.html)

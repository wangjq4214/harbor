# Widget Capability Plan

> Status: Foundation slices delivered; remaining desktop capabilities are planned
>
> Scope: Desktop-first capability planning for `crates/harbor-widget`. Product priority belongs to [`roadmap.md`](roadmap.md); [`next-stage-plan.md`](next-stage-plan.md) defines N01–N17, [`current-status.md`](current-status.md) records current status, and [`architecture/widget-runtime.md`](architecture/widget-runtime.md) defines the runtime contract.

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

| Area                  | Available now                                                                                                                                                     | Important limitation                                                                                                            |
| --------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------- |
| Declaration and state | Public `Component`/`View` construction, `view!`, `Fiber`, `Signal`, hooks, unique keyed sibling reorder                                                           | True dirty-subtree rebuilds remain incomplete; duplicate keys do not retain identity                                            |
| Layout                | Parent-directed Flex, `Row`/`Column`, `Expanded`, `Flexible`, `Spacer`, `ConstrainedBox`, `SizedBox`, `Padding`, `Align`, `Stack`, `FocusScope`, `LayoutObserver` | No positioned children, wrapping, grid, or general intrinsic/baseline protocol                                                  |
| Paint                 | Quads, rounded fills, borders, outer shadows, text runs, descendant clips, `CustomPaint`                                                                          | No image/SVG/path pipeline, transform, opacity layer, or general compositing primitives                                         |
| Text                  | `TextLabel`, shared `harbor-text` glyph metrics/cache                                                                                                             | Single-line cell-width-based measurement; no generic wrapping, selection, editing, rich spans, shaping, bidi, or ellipsis       |
| Input                 | Pointer capture and routing, `MouseRegion`, `InteractiveRegion`, `Focus`/`FocusHandle`, traversal/order, typed `Shortcuts`/`Actions`, IME preedit/commit routing  | Generic text editing, richer focus policies, drag-and-drop, and reusable drag/double-click recognizers remain open              |
| Controls and theme    | Themed `Button` with disabled state, text-glyph `IconButton`, shared interaction state, `ThemeProvider`; product `PreviewPane`                                    | No complete control families, forms, semantic tree, or general locale/direction environment                                     |
| Scrolling             | `ScrollArea`, `ScrollController`, `ScrollMetrics`, clipped viewport and focus reveal                                                                              | Generic scrollbar and virtual collections remain open                                                                           |
| Host effects          | Redraw, scheduling, cursor, IME, clipboard; optional winit host                                                                                                   | Terminal IME integration is not a generic `TextField`; further host services remain planned                                     |
| Quality               | Source and automated coverage for construction, layout, routing, rendering, effects, winit, and external paint                                                    | Coverage is not a current interactive smoke result; catalog examples, accessibility and visual-state matrices remain incomplete |

### Foundation evidence

- Construction and identity: [public construction](../crates/harbor-widget/src/construction.rs), [Component/View](../crates/harbor-widget/src/view.rs), [reconciliation](../crates/harbor-widget/src/fiber/reconcile.rs), [construction tests](../crates/harbor-widget/tests/child_construction.rs), and [macro tests](../crates/harbor-widget/src/macro_tests.rs).
- Layout: [Flex source](../crates/harbor-widget/src/widgets/flex.rs), [flex wrappers](../crates/harbor-widget/src/widgets/flexible.rs), [constrained sizing](../crates/harbor-widget/src/widgets/constrained_box.rs), [layout contract](flex-layout.md), [layout tests](../crates/harbor-widget/tests/flex_layout.rs), and [runtime tests](../crates/harbor-widget/tests/flex_runtime.rs). [LayoutObserver](../crates/harbor-widget/src/widgets/layout_observer.rs) has [committed-allocation tests](../crates/harbor-widget/tests/layout_observer.rs).
- Interaction and theme: [public widget modules](../crates/harbor-widget/src/widgets/mod.rs), [shared interaction](../crates/harbor-widget/src/widgets/interactive_region.rs), [theme](../crates/harbor-widget/src/theme.rs), and [interaction/focus/actions/theme tests](../crates/harbor-widget/tests/interaction_focus_actions_theme.rs).
- Scrolling: [ScrollArea/controller/metrics](../crates/harbor-widget/src/widgets/scroll_area.rs) and [scroll tests](../crates/harbor-widget/tests/scroll_area.rs).

`PreviewPane` and `CustomPaint` remain product integration widgets, not substitutes for generic text or input controls. Existing [product tabs and rail](../crates/harbor-app/src/ui.rs) do not complete a reusable navigation catalog. `IconButton` renders a text glyph, not an image/SVG resource pipeline. Theme inheritance is not a complete environment mechanism, and terminal IME support does not deliver a generic `TextField`.

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

| Capability                      | Flutter reference concepts                                                                | Harbor status                                                                                            | Planned Harbor surface                                                                                                            | Tier                   |
| ------------------------------- | ----------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------- | ---------------------- |
| Constraints and fixed sizing    | `ConstrainedBox`, `UnconstrainedBox`, `LimitedBox`, `FractionallySizedBox`, `AspectRatio` | Partial: `ConstrainedBox`, `SizedBox`, `BoxConstraints` delivered                                        | Add `AspectRatio`, fractional sizing; unconstrained layout only for a demonstrated case                                           | P0                     |
| Responsive composition          | `LayoutBuilder`, `MediaQuery`                                                             | Partial: committed `LayoutObserver` and product breakpoints                                              | Constraint-aware builder plus inherited window metrics; observation is not build-during-layout                                    | P0                     |
| Linear layout                   | `Flex`, `Row`, `Column`, `Expanded`, `Flexible`, `Spacer`                                 | Delivered foundation: parent-directed Flex, factors/fit/gaps, main/cross alignment, overflow diagnostics | Reuse the engine; extend intrinsic/baseline behavior only for concrete needs                                                      | P0                     |
| Overlay layout                  | `Stack`, `Positioned`, `Center`, `Align`                                                  | Partial                                                                                                  | Positioned/aligned stack children; keep `Center` as convenience rather than a new engine                                          | P0                     |
| Flow layout                     | `Wrap`                                                                                    | Missing                                                                                                  | `Wrap` with spacing, run spacing, and alignment                                                                                   | P1                     |
| Repeated two-dimensional layout | `GridView`, `Table`                                                                       | Missing                                                                                                  | Shared grid track solver, then finite `Grid` and `Table`; virtualized variants remain separate                                    | P2                     |
| Paint wrappers                  | `DecoratedBox`, `ClipRect`, `ClipRRect`, `Opacity`, `Transform`                           | Partial                                                                                                  | Explicit clip, opacity, transform, and repaint-boundary/layer decisions; preserve `DecoratedBox`                                  | P1                     |
| Images and icons                | `Image`, `Icon`, `RawImage`                                                               | Text-glyph `IconButton` only; resource pipeline missing                                                  | `Image` resource contract, fit/alignment, raster/SVG-or-path icon strategy, placeholder/error states                              | P2                     |
| Text display                    | `Text`, `RichText`, `SelectableText`                                                      | Minimal                                                                                                  | Proportional `Text`, wrap/max-lines/ellipsis/alignment, then spans and selectable text                                            | P1-P2                  |
| Pointer behavior                | `Listener`, `MouseRegion`, `GestureDetector`                                              | `MouseRegion`, cursor requests, `InteractiveRegion` delivered                                            | General pointer listener, click/double-click and desktop drag recognizers; no broad mobile gesture arena initially                | P0-P1                  |
| Focus and commands              | `Focus`, `FocusScope`, `FocusTraversalGroup`, `Shortcuts`, `Actions`                      | `Focus`/`FocusHandle`, order/enabled policy, scopes, typed `Shortcuts`/`Actions` delivered               | Roving focus, richer traversal policies, reusable default/cancel actions                                                          | P0                     |
| Semantics                       | `Semantics`, `ExcludeSemantics`, `MergeSemantics`                                         | Missing                                                                                                  | Role/name/value/state/action tree independent of painting, followed by Windows UI Automation bridge                               | P0 contract, P2 bridge |
| Scrolling                       | `Scrollable`, `ScrollView`, `SingleChildScrollView`, `Scrollbar`                          | `ScrollArea`, controller/metrics, viewport clipping, wheel/key routing and focus reveal delivered        | Generic scrollbar, explicit ensure-visible API as needed, extended scroll policies                                                | P1                     |
| Virtual collections             | `ListView.builder`, slivers, `GridView.builder`                                           | Missing                                                                                                  | `VirtualList` first; reusable viewport adapter for virtual grid/table/tree without cloning Flutter's sliver API                   | P2                     |
| Buttons                         | `TextButton`, `OutlinedButton`, `ElevatedButton`, `IconButton`                            | Themed `Button`/text-glyph `IconButton`, shared interaction and disabled state delivered                 | Additional style variants, general content slots, semantics                                                                       | P1                     |
| Selection controls              | `Checkbox`, `Radio`, `Switch`, `Slider`, segmented controls                               | Missing                                                                                                  | Checkbox, radio group, toggle/switch, slider, segmented button with keyboard and semantic behavior                                | P1                     |
| Text entry                      | `EditableText`, `TextField`, form fields                                                  | Missing                                                                                                  | Editing model and controller, caret/selection, clipboard, undo/redo, IME preedit, then `TextField` and validation shell           | P1-P2                  |
| Menus and choice                | `MenuBar`, `MenuAnchor`, `DropdownMenu`, popup menus                                      | Missing                                                                                                  | Menu model, menu bar, context menu, popup menu, combo box; integrated shortcuts and roving focus                                  | P1-P2                  |
| Overlay surfaces                | `Overlay`, `Tooltip`, `Dialog`, popup routes                                              | Missing                                                                                                  | Overlay root/entry, anchored placement, modal barrier, focus restore, Escape/default actions; compose tooltip/popover/dialog      | P1                     |
| Navigation and shell            | `Navigator`, `Scaffold`, tabs, navigation rail                                            | Product tabs/rail implemented; generic catalog incomplete                                                | Keep routing application-owned; reusable tabs, toolbar, sidebar/rail, breadcrumbs, split panes, status bar, and shell composition | P2                     |
| Data presentation               | `ListTile`, `Card`, `Divider`, `DataTable`                                                | `Separator` delivered; other surfaces mostly composable                                                  | Optional `Surface`/`ListRow` recipes, then sortable/resizable/selectable table                                                    | P2                     |
| Feedback                        | `ProgressIndicator`, badges, snack bars                                                   | Missing                                                                                                  | Determinate/indeterminate progress, inline status/badge, overlay-backed toast/notification                                        | P2                     |
| Animation                       | `Animation`, `Tween`, implicit/explicit transitions                                       | Scheduler only                                                                                           | Clock/ticker, animation controller, curves, reduced-motion policy, then a small transition set                                    | P2                     |
| Environment                     | `Theme`, `MediaQuery`, `Directionality`, localization                                     | `ThemeProvider` and control tokens delivered                                                             | General inherited environment, window metrics, text direction, locale/string lookup remain open                                   | P0-P2                  |
| Desktop host integration        | desktop window APIs beyond core Flutter widgets                                           | Partial effects/winit                                                                                    | Drag/drop, file picker and window commands as host services; never embed winit types in core widgets                              | P3                     |

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

**Delivered foundations**

- Public `Component`/`View` composition, child construction and `view!`; unique keyed sibling reorder preserves compatible identity. A separate `widgets::prelude` is not required to use the delivered surface.
- Parent-directed measurement and typed flex parent data; one Flex engine under `Row`/`Column`, factors, tight/loose fit, gaps, main-axis distribution, stretch, and diagnostics.
- `ConstrainedBox`, `Expanded`, `Flexible`, `Spacer`, and post-commit `LayoutObserver`.
- `MouseRegion`, `InteractiveRegion`, `Focus`/`FocusHandle` and order/enabled policy, typed `Shortcuts`/`Actions`, shared interaction state, and `ThemeProvider`.

Source and test links are listed under [Foundation evidence](#foundation-evidence); these are reusable foundations, not completion of every P0 acceptance criterion.

**Remaining runtime and API work**

- True dirty-subtree rebuilds remain follow-up work, not a prerequisite to rebuilding already delivered foundations.
- Extend environment contracts beyond theme to inherited window metrics, text direction, and localization.
- Define semantic nodes and the additional control-state contracts needed for selected/checked/invalid controls; connect existing interaction state rather than replacing it.

**Remaining layout and interaction work**

- Add aspect ratio, fractional sizing, and positioned stack children when required.
- Add a general pointer listener and reusable desktop drag/double-click behavior; extend traversal/roving focus and default/cancel actions for higher-level controls.

**Exit gate**

A resizable settings-style panel can be built without custom layout code; it has deterministic flex behavior, stable keyed state after reorder, complete keyboard traversal, theme-driven visuals, and a queryable semantic tree.

### P1 — Desktop Interaction Core

**Delivered foundations**

- `ScrollArea`, `ScrollController`, and `ScrollMetrics` supply generic viewport clipping, wheel/key handling and focus reveal; reuse their [scroll coverage](../crates/harbor-widget/tests/scroll_area.rs).
- `Button` and text-glyph `IconButton` already use shared interaction and theme styles, including disabled state; reuse the [interaction coverage](../crates/harbor-widget/tests/interaction_focus_actions_theme.rs).

**Remaining scrolling and overlays**

- Add generic desktop scrollbar behavior and extend ensure-visible/scroll policy where required; virtual collections remain P2.
- Add overlay entries, anchored placement with edge flipping/clamping, modal barriers, focus trapping/restoration, and Escape handling.
- Compose tooltip, popover, dialog, and notification surfaces from the overlay foundation.

**Text and controls**

- Upgrade text measurement and painting for proportional runs, wrapping, alignment, max lines, and ellipsis.
- Build the editing core: caret, selection, hit testing, clipboard, undo/redo, IME preedit/commit, candidate position, and horizontal scrolling.
- Extend existing themed button behavior with additional variants/content slots and semantics; add checkbox, radio, toggle, slider, and determinate progress.
- Add menu primitives needed for menu bars, context menus, and simple combo boxes.

**Exit gate**

A desktop dialog can contain themed labels, editable fields, toggles, a scrollable region, validation, default/cancel buttons, a tooltip, and a popup choice menu. It is fully operable by mouse and keyboard, preserves focus correctly, supports IME composition, and idles without redraw.

### P2 — Application Shell and Data Widgets

- Add `Wrap`, finite grid/table layout, toolbar, breadcrumbs, split panes, and status surfaces; reuse `Separator` and existing product tabs/rail without claiming a complete generic navigation catalog. Generalize tabs/sidebar recipes only where reuse requires it.
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

## Recommended Next Vertical Slice

Reuse the delivered construction, keyed identity, Flex, interaction, theme, focus/commands, scrolling, and layout-observation foundations. Product selection and sequencing belong to [`roadmap.md`](roadmap.md) and the N01–N17 requirements in [`next-stage-plan.md`](next-stage-plan.md), not to a restart of P0.

1. Build the generic single-line editing model and `TextField` needed by command-palette search and configuration UI: Unicode-safe selection/caret, clipboard, undo/redo, IME preedit/commit and candidate positioning.
2. Add overlay root/entries with anchored placement, modal routing, focus restoration, Escape/default actions; compose the command palette, popup choices, tooltips and configuration dialogs.
3. Compose settings fields and validation around existing themes, buttons, actions and `ScrollArea`; add selection controls and a generic scrollbar as the configuration UI requires them.
4. Add a reusable splitter with pointer capture/cancel, keyboard resizing, minimum-size policy and committed layout observation for panes. Keep terminal/session ownership in the application.
5. Define semantic contracts alongside new controls and retain the Windows UIA bridge as an explicit remaining gate, not implied by keyboard support.

The existing separate paste-confirmation window is not proof of a generic overlay system. These slices must earn their own source, automated, and Windows interactive evidence; they do not imply completion of the broader desktop catalog or runtime performance targets.

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

# Desktop Terminal Tabs and Declarative View Macro

**Spec ID:** 0010
**Status:** Proposed
**Date:** 2026-09-07

## Requirement

Harbor must support multiple independent terminal sessions in one main window, selected from a responsive vertical tab rail. Each tab retains its own Terminal and PTY lifecycle, while only the active tab receives input and terminal painting. The implementation must add the minimum reusable `harbor-widget` layout, interaction, styling, scrolling, and keyed-reconciliation capabilities required by this product slice.

Harbor must also provide an internal experimental Rust-style `view!(cx, ...)` procedural macro so nested widget trees, dynamic tab children, and conditional composition can be expressed without long fluent `.child(...)` chains. The macro is syntax sugar over ordinary widget constructors and the existing Component/View runtime; it must not introduce a second state, lifecycle, layout, or event model.

The initial target is Windows-first desktop behavior. Mobile navigation, touch-first tab gestures, and a stable third-party widget DSL are not required.

## Product Behavior

### Tab lifecycle

- The main window starts with one terminal tab created from the configured shell command and appearance.
- Creating a tab starts an independent Terminal, PTY, reader, screen, scrollback, selection, and cursor schedule.
- Activating a tab preserves every other tab's process and terminal state.
- Closing a tab shuts down and releases only that tab's PTY, reader, Terminal, bridge callbacks, and external registrations.
- Closing the active tab activates the tab to its right when one exists, otherwise the tab to its left.
- Closing the final tab closes the main window and exits the application; no replacement tab is created automatically.
- Tab titles initially use deterministic application-owned labels such as `Terminal 1`; dynamic OSC title integration is deferred.
- Tab drag reordering, pinning, persistence, grouping, and cross-window movement are deferred.

### Input and focus

- Only the active terminal receives keyboard, IME, pointer, wheel, selection, scrollbar, and paste input.
- Activating a tab restores focus to its terminal content unless focus intentionally remains on a tab-rail control.
- The tab rail is fully keyboard operable and exposes visible focus state.
- Default shortcuts are `Ctrl+T` for new tab, `Ctrl+W` for close active tab, `Ctrl+Tab` and `Ctrl+Shift+Tab` for next/previous tab, and `Ctrl+1` through `Ctrl+9` for direct activation.
- The existing paste confirmation window remains a separate OS window. Its application-owned gate blocks main-window terminal input and tab-changing commands while confirmation is active.

### Background activity and scheduling

- Inactive terminals continue ingesting and parsing PTY output.
- A background output event identifies its `TabId` so the Host can avoid treating every wake as active terminal paint demand.
- Background output may update a tab's unread indicator, but must not register an active cursor-blink loop or continuously repaint hidden terminal content.
- Only the active terminal contributes external draw presentation and visible cursor scheduling to the main Runtime.
- Activating, closing, or creating a tab atomically replaces the relevant active bridge registrations before the next frame.

### Resize and responsive layout

- The main content uses a horizontal flex layout: a bounded vertical tab rail, a separator, and an expanded terminal region.
- At widths of at least 900 logical pixels, the preferred rail width is 200dp and tab labels are visible.
- Below 900 logical pixels, the rail uses a compact 56dp presentation with icons/abbreviations and tooltip-accessible labels; it never becomes a mobile drawer or bottom navigation bar.
- The terminal allocation is the window content remaining after root inset, rail, separator, and decoration.
- A changed terminal allocation is converted to terminal rows/columns once and broadcast to every tab so inactive PTYs observe the same current viewport size.
- Allocation notifications are emitted only for actual geometry changes, are applied after layout rather than recursively during layout, and never send zero, negative, NaN, or duplicate PTY sizes.
- Zero-sized/minimized windows suspend drawing and do not issue invalid terminal resize requests.

## Widget Foundation

### Parent-directed layout

The current child-first layout contract must be extended so a parent can:

1. assign different constraints to individual children;
2. measure inflexible children before flexible children;
3. allocate remaining main-axis space;
4. remeasure a child when the parent's algorithm requires it;
5. associate lightweight typed parent data with a child; and
6. report bounded overflow without producing invalid geometry.

The first parent data contract covers flex factor/fit and positioned layout metadata. It must not become a general dynamically typed service channel.

### Required reusable widgets and mechanisms

| Capability | Required first-version surface | Product use |
| --- | --- | --- |
| Flex layout | `Flex`, horizontal `Row`, vertical `Column`, main/cross alignment, gap, overflow diagnostics | Rail + expanded terminal |
| Flexible child | `Flexible`, `Expanded`, `Spacer`, flex factor and loose/tight fit | Terminal receives remaining width |
| Constraints | `ConstrainedBox` with per-axis minimum/maximum bounds | Bound expanded/compact rail width |
| Separator | Horizontal/vertical convenience over existing box paint | Rail/content boundary |
| Interaction state | `InteractiveRegion` with disabled, hovered, pressed, focused, focus-visible, and selected states | Tab items and icon buttons |
| Pointer behavior | `MouseRegion` cursor/enter/exit and desktop click behavior | Rail hover and close affordances |
| Focus and commands | Node-level `Focus`, traversal order, `Shortcuts`, typed `Actions` | Keyboard tab operation |
| Styling | Inherited `Theme` tokens for colors, typography, spacing, radii, and interaction states | Remove hard-coded tab/button visuals |
| Scrolling | Vertical `ScrollArea`, controller/metrics, clipping, wheel, keyboard scroll, and ensure-visible | Overflowing tab rail |
| Layout observation | Post-layout allocation notification with equality coalescing | Broadcast terminal rows/columns |
| Keyed identity | Key wrapper/extension and sibling keyed reorder with duplicate-key diagnostics | Close/insert tabs without state migration |

A visible draggable `Scrollbar`, general `LayoutBuilder`, general gesture arena, animation system, image/icon pipeline, and overlay framework are not prerequisites for the first multi-tab delivery. `TabRail` and `TabItem` begin as application-level compositions over these reusable primitives and move into `harbor-widget` only after a second independent use demonstrates a generic contract.

## Declarative View Construction Contract

### Scope and ownership

`view!` is implemented in a new `harbor-widget-macros` proc-macro crate and re-exported by `harbor-widget`. The macro crate depends on parsing/quoting crates but not on `harbor-widget`, avoiding a dependency cycle. Generated paths resolve through a hidden `harbor_widget::__macro_support` module, with crate-rename handling where practical.

The macro is initially an internal experimental API. The architecture non-goal of a stable external DSL remains unchanged.

### Syntax

The macro accepts an explicit build context, an ordinary Rust widget-constructor expression, and `=>`-delimited children:

```rust
fn build(&self, cx: &mut BuildCx) -> View {
    view!(cx,
        Flex::horizontal().gap(1.0) => {
            ConstrainedBox::new().min_width(56.0).max_width(self.rail_width) => {
                TabRail::new() => {
                    ScrollArea::vertical() => {
                        for tab in self.tabs.iter() {
                            TabItem::new(tab.title.clone())
                                .selected(tab.id == self.active)
                                .on_select(self.select_tab(tab.id))
                                .keyed(tab.id)
                            => {}
                        }
                    }
                }
            }

            Separator::vertical() => {}

            Expanded::new() => {
                { self.active_bridge.clone() }
            }
        }
    )
}
```

`=>` is mandatory between a component expression and its child block so arbitrary Rust method chains and closures remain parseable without the macro knowing widget constructors or property metadata.

### Expansion semantics

- The root component is built with the supplied `BuildCx` and returns the concrete root `View` expected by `Component::build`.
- Child component expressions become deferred child Views.
- `{ expression }` converts an existing component/View/children value through explicit support traits without cloning or changing closure capture.
- `for`, `if/else`, and `match` normalize their produced children into ordered `View` values.
- Empty child blocks are valid.
- `keyed(...)` is an ordinary Rust extension method, not macro-only syntax.
- The macro preserves source spans and delegates widget constructor, method, callback, ownership, and lifetime errors to Rust whenever possible.
- The macro performs no implicit `clone`, `move`, state creation, hook invocation, event binding, or resource registration.

### Macro support API

`harbor-widget` provides a small non-rendering construction surface:

- `IntoChildView` converts a Component into a deferred `View` and passes an existing `View` through.
- `Children` collects ordered child Views from one value, an iterator, or control-flow expansion.
- `WithChildren` gives the macro one uniform attachment operation instead of guessing `.child` versus `.children` methods.
- `ComponentExt::keyed` wraps any component with a stable `Key` without exposing crate-private `AnyView`.
- `__macro_support` re-exports only the symbols generated code needs.

Single-child widgets reject more than one attached child with a diagnostic tied to the macro input or construction boundary. Multi-child widgets preserve source order. Existing handwritten fluent builders remain supported and behaviorally equivalent.

### Macro test contract

- Expansion tests cover leaf, single-child, multi-child, interpolation, nested control flow, empty lists, and stable keys.
- `trybuild` compile-fail cases cover malformed arrows/blocks, multiple roots, invalid interpolation, non-Component values, non-static borrowed callbacks, and invalid child cardinality where compile-time diagnosis is possible.
- Runtime equivalence tests compare macro and handwritten trees for widget type, key, Fiber order, layout, event routing, and scene output.
- `cargo expand` snapshots may aid review but are not the sole correctness evidence.

## Application Model and Boundaries

### Stable identifiers

```rust
struct TabId(u64);

struct TerminalTab {
    id: TabId,
    title: String,
    draw_id: ExternalDrawId,
    terminal: Arc<Mutex<Terminal>>,
    bridge: TerminalWidgetBridge,
}

struct TabManager {
    tabs: Vec<TerminalTab>,
    active: TabId,
    next_id: u64,
}
```

`TabId` is never reused during one application session. Each tab receives a distinct `ExternalDrawId`; `TerminalWidgetBridge` must no longer hard-code one global draw ID. The Host owns TabManager, TerminalTab, PTY lifecycle, shell creation, title/unread policy, and final-window close policy. `harbor-widget` owns only generic tree, layout, interaction, focus, theme, and scrolling behavior.

### Host events

The application event contract becomes tab-qualified:

```rust
enum AppEvent {
    TerminalOutputReady(TabId),
}
```

A wake for the active tab invalidates visible terminal output. A wake for an inactive tab drains/parses that terminal and updates only visible rail state when necessary. Stale events for already-closed TabIds are ignored safely.

### External draw registration

The active TerminalWidgetBridge is the only terminal bridge mounted for normal paint and input. Switching tabs removes the previous active external draw/schedule registrations and installs the next tab's stable registration. Bridge creation is tied to TerminalTab lifetime rather than repeated on every parent build.

### Terminal allocation propagation

A layout observer or equivalent post-layout effect reports the active terminal content allocation. TabManager converts that allocation with shared TextMetrics into a clamped TerminalSize and applies a changed value to every live tab. This mechanism must not expose Terminal types inside generic widget layout APIs.

## End-to-End Tests

### E2E: Create and independently use multiple tabs

- **Given:** The main window has one active terminal.
- **When:** The user creates a second tab and enters different commands in each tab.
- **Then:** Each command reaches only its active tab's PTY, and switching preserves independent screen, scrollback, cursor, and process state.

### E2E: Close tabs deterministically

- **Given:** Three tabs A, B, and C exist with B active.
- **When:** B is closed.
- **Then:** B's resources and registrations are released, C becomes active, A and C preserve keyed state, and stale B events are ignored.

### E2E: Close the final tab

- **Given:** One terminal tab remains.
- **When:** The user closes it through the button or `Ctrl+W`.
- **Then:** Its PTY shuts down and Harbor closes the main window rather than creating another tab.

### E2E: Resize all sessions from the content allocation

- **Given:** Multiple active and inactive tabs exist.
- **When:** The window is resized, minimized/restored, or moved across 100%, 150%, and 200% DPI displays.
- **Then:** The rail follows its expanded/compact policy, the terminal region receives the remaining valid allocation, and every live PTY receives each changed row/column size once.

### E2E: Background output does not cause hidden rendering

- **Given:** An inactive tab produces sustained output while the active tab is idle.
- **When:** Host events are processed.
- **Then:** The inactive Terminal ingests output, optional unread state changes, no hidden terminal draw occurs, and the event loop does not remain in Poll solely because of the inactive cursor.

### E2E: Keyboard and paste modal behavior

- **Given:** Focus is in the active terminal or tab rail.
- **When:** Tab shortcuts and focus traversal are used, including while paste confirmation is open.
- **Then:** Commands activate the expected tab and restore focus, while the existing confirmation gate blocks tab-changing and terminal-input commands until confirmation completes.

### E2E: Macro and handwritten composition are equivalent

- **Given:** Equivalent static and dynamic tab-rail trees are authored once with fluent builders and once with `view!`.
- **When:** They are built, reconciled, laid out, painted, and sent input.
- **Then:** Their keys, Fiber order, geometry, event targets, and scene output are equivalent.

## Decisions

### Deliver one product slice rather than a complete widget catalog

- **Choice:** Implement only the generic widget capabilities directly required by side-rail terminal tabs.
- **Reason:** This proves each abstraction through a concrete Harbor use case and avoids recreating Flutter's mobile-oriented catalog.

### Keep tab/session ownership in the Runtime Host

- **Choice:** `TabManager` owns TerminalTab and PTY lifecycles in the application layer; `harbor-widget` receives generic composition and callbacks only.
- **Reason:** The Runtime Host already owns shell startup, windows, GPU resources, paste policy, and fatal lifecycle handling.
- **ADR reference:** [0011-terminal-custompaint-gpu-injection](../adr/0011-terminal-custompaint-gpu-injection.md), [0015-runtime-owned-frame-presentation](../adr/0015-runtime-owned-frame-presentation.md)

### Use a vertical responsive rail, not mobile navigation

- **Choice:** Use a persistent 200dp/56dp side rail selected by available logical width.
- **Reason:** Mouse, keyboard, resize, and wide-window use are the product target; a drawer or bottom bar would introduce unrelated mobile interaction policy.

### Use a thin internal Rust-style procedural macro

- **Choice:** `view!(cx, expression => { children })` expands ordinary constructors and child attachment through public support traits.
- **Reason:** It improves tree readability while retaining Rust ownership/type errors and one Component/View runtime model.

### Keep macro keying explicit

- **Choice:** Dynamic items call the ordinary `.keyed(TabId)` extension; the macro never invents positional or data-derived keys.
- **Reason:** Identity is application policy, and hidden key generation would obscure reconciliation behavior.

### Close the main window with the final tab

- **Choice:** Closing the final terminal tab terminates its session and closes Harbor.
- **Reason:** This is the selected terminal-style lifecycle and avoids hidden automatic shell creation.

## Test Plan

- **Widget layout tests:** Per-child constraints, flex factors/fit, gaps, bounded rail widths, zero/unbounded constraints, overflow, nested flex, repeated measurement, fractional coordinates, and DPI transitions.
- **Reconciliation tests:** Keyed insert/delete/reorder, duplicate keys, focus and hook preservation, stale generations, and external registration replacement.
- **Interaction tests:** Hover/press/cancel, primary/secondary click policy, focus-visible, Tab traversal, shortcut conflicts, selected/disabled states, and ensure-visible.
- **Macro tests:** Parser/expansion unit tests, `trybuild` pass/fail fixtures, control flow, spans, crate rename behavior, and runtime equivalence.
- **Tab model tests:** Add/activate/close selection policy, final-tab close outcome, stable IDs, stale events, independent Terminal state, resource shutdown, and title/unread policy.
- **Scheduling tests:** Active versus inactive output, active-only external schedule, cursor blink, synchronized output, close cleanup, and quiet-runtime Wait behavior.
- **Resize tests:** Content allocation to TerminalSize conversion, broadcast deduplication, inactive tabs, zero size, root inset, rail breakpoints, maximize/restore, and scale changes.
- **Manual Windows tests:** Create and close many tabs; run shell, Vim, less, and sustained output in separate tabs; exercise mouse, keyboard, selection, scrollback, paste confirmation, Acrylic, resize, monitor DPI movement, suspend/resume, and shell exit.
- **Quality gates:** Follow [`docs/validation.md`](../../docs/validation.md), including format, clippy, workspace tests, documentation checks, and Windows runtime evidence for input/window behavior.

## Performance and Resource Gates

- Hidden tabs produce no terminal draw calls and no cursor-driven Poll scheduling.
- Tab rail layout and hit testing are linear in visible tab items; no spatial index is introduced without profiling evidence.
- ScrollArea does not require full virtual-list infrastructure for the initial expected tab count, but must clip painting and avoid work proportional to terminal cell count for hidden tabs.
- Closing a tab removes its Fiber subtree, external registrations, schedule provider, reader, PTY control, and Terminal ownership without targeting a reused generation.
- Resize broadcasts are coalesced by TerminalSize and do not repeatedly resize PTYs for unchanged logical allocations.
- The existing active-terminal frame target and idle behavior remain within the current validation/performance policy.

## Out of Scope

- Cupertino widgets, bottom navigation, drawers, swipe navigation, touch-first gestures, safe-area/mobile keyboard layout, and mobile adaptive controls.
- Drag-reordered, pinned, grouped, persisted, restored, detachable, or cross-window tabs.
- Multiple main windows, system tray behavior, docking, split terminal panes, and general in-window routing.
- Dynamic OSC tab titles, shell icons, profile pickers, and user-configurable tab styling.
- Full Flutter-compatible Flex, Sliver, gesture, animation, theme, semantics, or accessibility APIs beyond the contracts directly required here.
- A stable external DSL, `#[component]`, generated Props, hooks syntax, CSS, RSX/HTML syntax, reflection, hot reload, automatic clone/move, or automatic keys.
- General LayoutBuilder, virtual lists, draggable Scrollbar, rich text, image/path icons, and a general Overlay system.

## Future Evolution

- Move TabRail/TabItem into `harbor-widget` after a second application use confirms reusable behavior and styling.
- Add draggable scrollbars or virtualized tab children only when measured tab counts require them.
- Add dynamic titles and profile metadata after OSC permission/title policy and configuration UX are specified.
- Add drag reorder only after keyed reconciliation, drag/drop, focus movement, and close-target behavior are proven independently.
- Generalize the post-layout observer only after another host service needs allocation feedback.
- Consider a stable external macro API only after handwritten and macro-authored Harbor screens establish durable construction conventions.

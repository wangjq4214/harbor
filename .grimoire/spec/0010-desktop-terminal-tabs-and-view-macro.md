# Desktop Terminal Tabs and Declarative View Macro

**Spec ID:** 0010
**Status:** In Progress
**Date:** 2026-09-07

## Requirement

Harbor must support multiple independent terminal sessions in one main window, selected from a responsive vertical tab rail. Each tab retains its own Terminal and PTY lifecycle, while only the active tab receives input and terminal painting. The implementation must add the minimum reusable `harbor-widget` layout, interaction, styling, scrolling, and keyed-reconciliation capabilities required by this product slice.

Harbor must also provide an internal experimental Rust-style `view! { cx; root }` procedural macro so nested widget trees, dynamic tab children, and conditional composition can be expressed without long fluent `.child(...)` chains. The macro is syntax sugar over ordinary Rust expressions and the existing Component/View runtime; it must not introduce a second state, lifecycle, layout, or event model.

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
| Interaction state | `InteractiveRegion` with disabled, hovered, pressed, focused, focus-visible, and selected states; pointer capture is released on pointer-up, cancel, focus loss, disabling, unmount, and window-lifecycle cancellation | Tab items and icon buttons |
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

The macro accepts exactly one root after a context expression separated by `;`. Ordinary child values are Rust expressions terminated by `;`; child-bearing components use `expression => { children }`; bare `for`, `if`/`else`, and `match` organize child lists:


Rust blocks are ordinary expressions and require a trailing semicolon when used as child values. `=>` is reserved for components that receive a braced child list. Parenthesized or block-wrapped `if` and `match` retain ordinary Rust value-expression semantics.
### Expansion semantics

- A root value is a Component built directly with the supplied `BuildCx`; an already-built `View` is supported only as a child value.
- `expression;` evaluates once and converts through `IntoChildView`, preserving deferred Component children and existing Views.
- `expression => { children }` constructs children first, attaches them through `WithChildren`, then converts or builds the component according to root/child position.
- Bare `for`, `if/else`, and `match` write selected values into the current ordered `Children` collector; collections are expanded explicitly with a loop.
- Empty parent child lists are valid, while value statements do not call `WithChildren` with an empty list.
- `keyed(...)` is an ordinary Rust extension method, not macro-only syntax.
- The macro preserves source spans and delegates ordinary expression, method, trait, ownership, and lifetime errors to Rust whenever possible.
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

- Parser and expansion tests cover leaf and parent roots, value and parent children, Rust blocks, struct literals, wrapped value control flow, bare child-list control flow, empty lists, and stable generated names.
- `trybuild` pass/fail cases cover expression forms, renamed dependencies, the old separator/interpolation rejection, malformed terminators and child blocks, multiple roots, trailing semicolons, non-Component values, and the Component-only root bound.
- Runtime equivalence tests compare macro and handwritten trees for widget type, key, Fiber order, layout, event routing, and scene output.
- `cargo expand` snapshots may aid review but are not the sole correctness evidence.

## Application Model and Boundaries

### Stable identifiers


`TabId` is never reused during one application session. Each tab receives a distinct `ExternalDrawId`; `TerminalWidgetBridge` must no longer hard-code one global draw ID. The Host owns TabManager, TerminalTab, PTY lifecycle, shell creation, title/unread policy, and final-window close policy. `harbor-widget` owns only generic tree, layout, interaction, focus, theme, and scrolling behavior.

### Host events

The application event contract becomes tab-qualified:

Output wakes are qualified by stable TabId.

A wake for the active tab invalidates visible terminal output. A wake for an inactive tab drains/parses that terminal and updates only visible rail state when necessary. Stale events for already-closed TabIds are ignored safely.

### External draw registration

The active TerminalWidgetBridge is the only terminal bridge mounted for normal paint and input. Switching tabs removes the previous active external draw/schedule registrations and installs the next tab's stable registration. Bridge creation is tied to TerminalTab lifetime rather than repeated on every parent build.

### Terminal allocation propagation

A layout observer or equivalent post-layout effect reports the active terminal content allocation. TabManager converts that allocation with shared TextMetrics into a clamped TerminalSize and applies a changed value to every live tab. This mechanism must not expose Terminal types inside generic widget layout APIs.

## End-to-End Tests

Verify independent sessions and active-only input/paint; close B among A/B/C selects C and preserves A/C keyed state; final-tab close shuts down the PTY and exits without replacement. Resize/minimize/restore and 100/150/200% DPI transitions must apply each changed valid grid once to all live PTYs. Hidden output must progress without hidden draws or cursor-driven Poll. Keyboard/rail focus and paste gating must work from either focus origin. Macro and handwritten trees must match keys, Fiber order, geometry, routing and scene output.

## Test Plan

- **Widget layout tests:** Per-child constraints, flex factors/fit, gaps, bounded rail widths, zero/unbounded constraints, overflow, nested flex, repeated measurement, fractional coordinates, and DPI transitions.
- **Reconciliation tests:** Keyed insert/delete/reorder, duplicate keys, focus and hook preservation, stale generations, and external registration replacement.
- **Interaction tests:** Hover/press/cancel, primary/secondary click policy, focus-visible, Tab traversal, shortcut conflicts, selected/disabled states, and ensure-visible.
- **Macro tests:** Parser/expansion unit tests, `trybuild` pass/fail fixtures, control flow, spans, crate rename behavior, and runtime equivalence.
- **Tab model tests:** Add/activate/close selection policy, final-tab close outcome, stable IDs, stale events, independent Terminal state, resource shutdown, and title/unread policy.
- **Scheduling tests:** Active versus inactive output, active-only external schedule, cursor blink, synchronized output, close cleanup, and quiet-runtime Wait behavior.
- **Resize tests:** Content allocation to TerminalSize conversion, broadcast deduplication, inactive tabs, zero size, root inset, rail breakpoints, maximize/restore, and scale changes.
- **Manual Windows tests:** Create and close many tabs; run shell, Vim, less, and sustained output in separate tabs; exercise mouse, keyboard, selection, scrollback, paste confirmation, Acrylic, resize, monitor DPI movement, suspend/resume, and shell exit.
- **Quality gates:** Follow validation policy, including format, clippy, workspace tests, documentation checks, and Windows runtime evidence for input/window behavior.

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

## Outstanding Acceptance

Historical 0010/T0008 is retained here as an acceptance obligation, not a passed gate. Verify activation replaces draw/schedule registrations before presentation; close removes callbacks, reader, PTY control and ownership without stale-generation targeting. Exercise failure handling and surface recovery as well as happy-path resize and final close. Run the format, clippy, workspace-test and documentation/checklist gates in the validation policy. Record real Windows shell/Vim/less/sustained-output, shortcuts, selection, scrollback, Acrylic, resize, DPI and minimize/restore evidence and quiet/active-frame/resource measurements. Unrun checks remain unrun; discovered failures require explicit follow-up, not silent exceptions.

Historical 0010/T0001–T0007 map respectively to parent-directed layout, keyed construction/reconciliation, the view macro, interaction/focus/actions/theme, scrolling/allocation observation, tab/session ownership, and responsive rail behavior defined above.

# Context

Project domain concepts and terminology. Linked ADRs/specs retain detailed policy and evidence; definitions do not establish implementation or runtime acceptance.

## Domain Index

| Domain | File | Entry Count |
| --- | --- | ---: |
| Terminal Protocol | [CONTEXT-terminal-protocol.md](./CONTEXT-terminal-protocol.md) | 41 |

## Concepts

### System-Native Font Loading
- **Definition:** A `harbor-text` strategy in which system font discovery and font data loading use operating-system APIs to avoid copying complete font files into the Rust heap.

### System Default Font Selection
- **Definition:** The Windows `harbor-text` policy that selects a configured TOML font family when available and otherwise delegates primary font selection to the operating system.
- **Relationships:**
  - depends on System-Native Font Loading

### Terminal Font Policy
- **Definition:** Configured family and size govern terminal rendering/grid metrics, not widget, chrome or dialog fonts. Live changes preserve application/session/PTY identity and buffers, using ordinary bounded resize rather than lossless history retention or rejection for normal eviction/clipping. See [ADR 0059](./adr/0059-atomic-manual-reload-and-terminal-only-fonts.md) and [ADR 0060](./adr/0060-font-reload-uses-normal-bounded-resize.md).
- **Relationships:**
  - depends on System Default Font Selection

### DirectWrite Font Backend
- **Definition:** The Windows `harbor-text` backend that uses DirectWrite for font selection, system fallback, metrics, glyph resolution, and rasterization while retaining the existing WGPU glyph atlas.
- **Relationships:**
  - implements System-Native Font Loading

### System Font Fallback
- **Definition:** The DirectWrite-based policy that resolves missing characters through Windows fallback fonts for both environment-overridden and system-selected primary fonts.
- **Relationships:**
  - belongs to DirectWrite Font Backend

### Emoji Sequence Presentation
- **Definition:** DirectWrite whole-unit shaping and Direct2D offscreen tiles feed a separate bounded wgpu color atlas; ordinary text retains its R8 path. Uniform fitting/clipping never changes model-assigned width or copied text. Complete color, complete monochrome and unsupported fallback are distinct outcomes; see [ADR 0054](./adr/0054-color-emoji-offscreen-presentation.md).
- **Relationships:**
  - depends on DirectWrite Font Backend
  - references System Font Fallback

### Harbor Widget Runtime
- **Definition:** Declarative Rust/wgpu UI runtime from reactive component state through reconciliation, layout and retained GPU scene encoding.
- **Synonyms:** Widget Runtime, Runtime
- **Relationships:**
  - contains Fiber
  - contains RenderNode
  - contains Signal
  - contains View

### Reusable Widget Component
- **Definition:** A `harbor-widget` mechanism meaningful in a non-terminal application without interpreting Harbor business types. Consumer count and icon names do not determine ownership; product composition, command meaning and policy remain application-owned. See [ADR 0064](./adr/0064-widget-and-application-component-ownership.md).
- **Relationships:**
  - belongs to Harbor Widget Runtime

### Widget Text Input
- **Definition:** Reusable single-line input in `harbor-widget`, separating coherent controller-owned text/selection/composition state, a decoration-free editing core and a decorated field. Search commands, result navigation and query limits remain application-owned. See [ADR 0065](./adr/0065-layered-widget-text-input.md).
- **Relationships:**
  - belongs to Harbor Widget Runtime
  - implements Reusable Widget Component

### View
- **Definition:** An immutable UI description produced by component build methods, discardable after each build pass.
- **Synonyms:** Widget

### Fiber
- **Definition:** Long-lived component identity retaining hooks/state, Signal subscriptions, child fibers and dirty flags across rebuilds.
- **Synonyms:** Fiber Node

### RenderNode
- **Definition:** Retained layout, transform, clip, paint-order and hit-test geometry associated with a Fiber and consumed by the Scene Graph.

### Signal
- **Definition:** UI-thread pull-based reactive state: writes dirty subscribed fibers rather than pushing values through the tree.
- **Synonyms:** State Cell

### Key
- **Definition:** A stable identity marker used during reconciliation to match views between build passes and preserve fiber state.

### Reconciliation
- **Definition:** The process of diffing the new View tree against the existing Fiber tree, reusing fibers where type and Key match, and destroying mismatched subtrees.
- **Synonyms:** Reconciler

### BoxConstraints
- **Definition:** Parent-imposed child size intervals used by bounded parent-directed measurement and atomic geometry commit. Infinity means an unbounded maximum, never a measured extent. Legacy constrain is min-wins; ConstrainedBox enforces local bounds inside the authoritative parent interval. See [flex layout](../docs/flex-layout.md).

### Generation Arena
- **Definition:** A slotmap-based array where each slot has a generation counter; stale references (FiberId) are detected by generation mismatch on access.
- **Synonyms:** SlotMap

### Component
- **Definition:** Stateless builder: `build(&mut BuildCx)` returns an immutable View; Fiber owns lifecycle state.
- **Synonyms:** Widget Builder

### BuildCx
- **Definition:** Component build context creating hook-backed Signals and tracking their dependencies during reconciliation.
- **Relationships:**
  - belongs to Reconciliation

### AnyView
- **Definition:** Internal type-erased View contract exposing reconciliation identity and the next build result.

### Hook
- **Definition:** Per-Fiber Signal slot preserved by use_state call order through BuildCx.

### Primitive
- **Definition:** A standardized retained draw input produced during widget painting. Variants cover Quad, Text, Icon, Border, OuterShadow, RoundedQuad, RoundedBorder, and External delegate rendering; a primitive is input to scene encoding rather than a guarantee of exactly one GPU draw call.

### SceneItem
- **Definition:** A retained GPU-visible draw item containing a stable ID, Primitive, ancestor clip stack, and paint-order index.

### SceneDelta
- **Definition:** An incremental update describing added, removed, or modified SceneItems since the last frame; consumed by the widget renderer to update GPU buffers.

### Scene Graph
- **Definition:** A retained flat ordered list of SceneItems sorted by paint order, enabling incremental GPU updates without rebuilding vertex buffers every frame.
- **Synonyms:** Retained Scene
- **Relationships:**
  - belongs to Harbor Widget Runtime

### Widget Icon
- **Definition:** A non-interactive `harbor-widget::Icon` leaf painting built-in Lucide vector artwork independently of text metrics/fonts, with logical sizing, tint, DPI-aware antialiasing and ancestor clipping. It is not an arbitrary SVG loader; controls/application own interaction and tab policy. See [ADR 0057](./adr/0057-widget-vector-icons-without-font-glyphs.md).
- **Synonyms:** Icon, Lucide icon
- **Relationships:**
  - belongs to Harbor Widget Runtime
  - depends on Widget Renderer

### Widget Renderer
- **Definition:** Independent wgpu instanced-quad pipeline encoding SceneDelta into a shared render pass; owns its GPU buffers.
- **Relationships:**
  - belongs to Harbor Widget Runtime

### Layout Container
- **Definition:** Positions child Views: Padding insets, Row/Column flex, Stack overlays and Align positions within the parent.
- **Synonyms:** Container Widget

### Padding Widget
- **Definition:** Zero-/one-child container deducting insets before child layout and adding them back to its own size.
- **Synonyms:** Padding

### Widget Child Contract
- **Definition:** The agreed construction capability classification: Leaf has no attachable slot, SingleChild has zero or one child, and MultiChild has an ordered zero-or-more child list. Setters and macro child blocks replace existing content; an empty block clears it, while explicitly named append operations preserve existing content then append in input order. Static violations fail compilation where provable; dynamic violations return ChildConstructionError in recoverable handwritten construction and panic at construction in ordinary view!. See [ADR 0080](./adr/0080-unify-child-cardinality-and-replacement-semantics.md); this is a selected contract, not completed implementation.
- **Relationships:**
  - belongs to Harbor Widget Runtime
  - references Widget View Macro
  - references Layout Container

### Widget Decoration
- **Definition:** A general Harbor Widget Runtime visual capability that paints a widget's box without changing its measured size or layout bounds.

### Widget Decoration Paint
- **Definition:** Reusable `harbor-widget::Paint` for solid colors and linear-gradient box fills/border strokes. Rendering and alpha composition are widget-owned; material presets are application-owned, and existing solid-color APIs remain compatible. See [ADR 0067](./adr/0067-widget-gradient-paints-with-application-material-presets.md).

### Widget Local Backdrop Blur
- **Definition:** Reusable `harbor-widget::BackdropFilter` that softens already-drawn content, including embedded terminal presentation, while leaving its foreground sharp. Unlike Windows Acrylic it filters within the window; product presets remain application-owned. Search and the command palette share code-level appearance defaults with per-consumer overrides, not new TOML settings; see [ADR 0070](./adr/0070-shared-application-glass-presets-with-local-overrides.md). See [ADR 0066](./adr/0066-widget-local-backdrop-blur-with-application-glass-presets.md).

### Widget Backdrop Edge Refraction
- **Definition:** Optional widget-owned local background displacement concentrated near edges and rounded corners, fading to zero in the center while leaving foreground and hit testing unchanged. Application-owned glass recipes choose its parameters; blur-only consumers keep existing behavior. See [ADR 0068](./adr/0068-local-backdrop-edge-refraction.md).

### Native Glass Material
- **Definition:** Selected native Rust/wgpu adaptation of Glass-HQ/liquid-glass material algorithms over in-window content, extending widget-owned backdrop rendering while retaining application-owned presets. Edge displacement, outline-normal highlights/side shading and blur/tone processing are the material scope, not the upstream web control or motion system. [ADR 0076](./adr/0076-native-liquid-glass-material-port.md) records the direction; initial native implementation exists, while full visual acceptance remains separate.
- **Relationships:**
  - extends Widget Backdrop Edge Refraction
  - depends on Widget Local Backdrop Blur
  - references Windows Acrylic Backdrop

### DecoratedBox
- **Definition:** A general wrapper widget that applies BoxDecoration and ClipBehavior to one child without adding Terminal-specific visual properties.
- **Relationships:**
  - belongs to Harbor Widget Runtime

### BoxDecoration
- **Definition:** Logical-pixel box styling: optional fill (none by default), uniform Border, BorderRadius and layered BoxShadows; child clipping is separate.
- **Relationships:**
  - contains BorderRadius
  - contains BoxShadow

### BorderRadius
- **Definition:** A box geometry value supporting both a uniform circular radius and independent radii for each of the four corners.

### BoxShadow
- **Definition:** An outer box shadow value with color, offset, non-negative blur radius, and optionally negative spread radius that can be layered without affecting layout.

### Border
- **Definition:** A first-version box outline value that applies one color and non-negative logical-pixel width uniformly to all four edges.

### ClipBehavior
- **Definition:** An explicit `None`, `HardEdge`, or `AntiAlias` widget policy that applies rounded bounds consistently to child painting and hit testing while leaving shadows outside the clip.

### Decoration Paint Order
- **Definition:** A stable widget painting sequence that draws shadows, background, child content, and border in that order.
- **Relationships:**
  - belongs to Widget Decoration
  - references BoxDecoration

### Portal
- **Definition:** A root-composable widget escape: declared content appears in a root overlay outside its parent's layout/clipping without consuming terminal space. Notification policy is application-owned. See [ADR 0063](./adr/0063-root-composable-escape-and-quiet-configuration-notices.md) and [ADR 0064](./adr/0064-widget-and-application-component-ownership.md).
- **Synonyms:** Escape component, Root-level escape component
- **Relationships:**
  - belongs to Harbor Widget Runtime

### Notification Presentation
- **Definition:** Reusable widget card/content and close/scroll interaction, with local pressed/scroll state; application-owned placement, appearance, eligibility, message meaning, deduplication, recovery and dismissal history. Harbor notices remain top-right, non-modal, manually dismissible, without focus theft; all Harbor floating notices, including actionable errors, automatically close after three seconds rather than remaining permanent. See [ADR 0064](./adr/0064-widget-and-application-component-ownership.md) and the lifetime override in [ADR 0071](./adr/0071-three-second-notifications-including-errors.md).
- **Relationships:**
  - depends on Portal
  - references Reusable Widget Component

### Viewport
- **Definition:** The logical pixel size, physical pixel size, and scale factor passed to Runtime::encode for converting dp layout coordinates to GPU NDC.

### Runtime Host
- **Definition:** The combined hosting boundary for a native Widget Runtime, split between reusable infrastructure in `harbor-widget::winit` and application-owned event-loop, business, and process policy.
- **Synonyms:** Native Widget Host
- **Relationships:**
  - contains Winit Adapter
  - contains Application Business Host
  - contains Harbor Widget Runtime

### Application Business Host
- **Definition:** The binary application layer that owns EventLoop/ApplicationHandler coordination, multi-window routing, application models and reducers, platform-specific window policy, and fatal/exit decisions while delegating generic window, Runtime, wgpu, presentation, and HMR lifecycle to the Winit Adapter.
- **Synonyms:** Application Host, App
- **Relationships:**
  - communicates with Harbor Widget Runtime

### Application UI Composition
- **Definition:** Application-owned workspace/root, tab, notification, confirmation/shared PreviewPane and product-appearance composition. Notification state/keybinding publication are application-level, not tab-owned; `harbor-app-ui` stays a thin HMR root. These are responsibility groups, not mandated modules/types/stores; see [ADR 0064](./adr/0064-widget-and-application-component-ownership.md).
- **Relationships:**
  - communicates with Application Business Host
  - depends on Reusable Widget Component
  - references Terminal Widget Bridge

### Desktop Pointer Cursor Policy
- **Definition:** Harbor's selected desktop interaction convention chooses mouse cursor shapes by interaction meaning, not clickability: ordinary buttons (including close/new controls), tabs and menu items keep the default arrow; hyperlinks use the hand pointer; text-editing regions use the I-beam; resize and drag regions use their corresponding specialized cursors. Hover highlighting and pressed feedback communicate ordinary controls' clickability instead of a hand cursor. This records the agreed convention, not completed implementation or runtime acceptance.

### Application-Layer Widget Hot Reload
- **Definition:** An optional Windows debug-development capability in the `harbor-widget::winit` integration that observes reloads, tears down the active root safely, and installs a replacement application root while retaining native host infrastructure and Host-owned business state.
- **Synonyms:** Widget HMR, UI hot reload
- **Relationships:**
  - belongs to Winit Adapter
  - communicates with Harbor Widget Runtime

### Winit Adapter
- **Definition:** A feature-gated `harbor-widget` native-host adapter and builder that owns per-window creation, Surface and Runtime lifetimes, generic wgpu initialization, event conversion, scheduling, presentation, and optional HMR root replacement while accepting application-provided attributes and platform hooks.
- **Synonyms:** Winit Event Adapter, Native Widget Adapter
- **Relationships:**
  - contains Harbor Widget Runtime

### WinitFrameTarget
- **Definition:** The former frame-scoped borrowed Window, Surface, Device, and Queue bundle used when the binary application owned native resources; under the adapter-owned host boundary it is removed from the application contract and may remain only as an internal implementation detail.
- **Relationships:**
  - belongs to Winit Adapter

### Runtime Frame Presentation
- **Definition:** Winit Adapter-owned surface acquisition, GPU encode/submit, window notification and presentation.
- **Relationships:**
  - belongs to Winit Adapter
  - extends Widget Renderer

### Runtime Integration Boundary
- **Definition:** Feature-gated generic winit/wgpu lifecycle and optional root HMR API; excludes terminal, paste, backdrop and Harbor business policy.
- **Relationships:**
  - contains Winit Adapter
  - contains Runtime Frame Presentation

### External Runtime Invalidation
- **Definition:** Host-asynchronous dirty signal for the runtime scheduler, without exposing source-specific business event types.

### TerminalOutputReady
- **Definition:** Host event announcing PTY-reader output ready for UI-thread processing; produces External Runtime Invalidation.
- **Relationships:**
  - belongs to Application Business Host

### Runtime Frame Scheduler
- **Definition:** The runtime-owned state machine that converts invalidation, animation deadlines, and steady-state activity into Wait, WaitUntil, Poll, and RequestRedraw effects.
- **Relationships:**
  - belongs to Harbor Widget Runtime

### Terminal Input Semantics
- **Definition:** Terminal-owned interpretation of routed platform-independent keyboard/pointer input, including selection, scrolling and scrollbar control.
- **Relationships:**
  - implements Terminal Pointer Interaction Policy

### Terminal Pointer Interaction Policy
- **Definition:** Harbor routes pointer input to terminal selection and scrollback controls unless an application has enabled VT mouse reporting, which takes precedence and receives the input through the PTY.
- **Relationships:**
  - belongs to Terminal Input Semantics

### Terminal Text Selection
- **Definition:** A left-button drag selects terminal cells across lines and auto-scrolls at the viewport edge while preserving the resulting highlight after release.
- **Synonyms:** text selection, selection
- **Relationships:**
  - depends on Terminal Pointer Interaction Policy
  - implements Terminal Selection Lifecycle
  - implements Terminal Selection Granularity

### Terminal Selection Copy Policy
- **Definition:** Ctrl+Shift+C always copies (empty text without selection); Ctrl+C copies a selection or passes interrupt to PTY. Successful copy clears selection.
- **Relationships:**
  - belongs to Terminal Text Selection

### Terminal Selection Lifecycle
- **Definition:** Copy preserves explicit newlines and joins soft wraps, then clears selection. New left press replaces it; output, scrolling and focus loss retain it. Escape cancels selection/local pointer interaction only when present; otherwise it reaches PTY, including alternate apps.
- **Relationships:**
  - belongs to Terminal Text Selection
  - references Logical Line
  - references Soft-Wrap Marker

### Terminal Selection Granularity
- **Definition:** Drag selects cells; double-click selects Unicode alphanumeric/underscore words; triple-click selects a Logical Line without crossing explicit newlines.
- **Relationships:**
  - references Logical Line

### Terminal Scrollbar
- **Definition:** The terminal scrollback control supports wheel scrolling, draggable-thumb positioning, and track-click page movement while preserving a user's review position as new output arrives.
- **Synonyms:** scrollbar
- **Relationships:**
  - depends on Terminal Pointer Interaction Policy

### RuntimeEffects
- **Definition:** A platform-independent command batch returned by Runtime for its Host to apply, including redraw, cursor, IME, and clipboard requests.
- **Synonyms:** Runtime Effects

### Runtime Per Window
- **Definition:** The ownership model in which each OS window has an independent Runtime, Widget tree, and input state while GPU and text resources may be shared.
- **Relationships:**
  - depends on Runtime Host
  - contains Harbor Widget Runtime

### UiEvent
- **Definition:** A single enum representing all input events (pointer, keyboard, focus) dispatched through the widget tree via capture-target-bubble routing.
- **Synonyms:** Event

### EventCtx
- **Definition:** Deferred event-handler commands for focus, pointer capture/release, paint invalidation and propagation; applied after routing.

### FocusScope
- **Definition:** A widget that wraps a subtree and manages Tab/Shift+Tab focus traversal within it; supports a modal flag that blocks events from reaching widgets outside the scope.
- **Relationships:**
  - implements AnyView

### Hit Testing
- **Definition:** Reverse-paint-order traversal of the Render Tree checking point-in-rect per widget, used to determine the event target for pointer events.

### Pointer Capture
- **Definition:** A mechanism where a widget that receives a pointer-down can request to receive all subsequent move/up/cancel events for that pointer, even if the pointer moves outside its bounds.

### InputState
- **Definition:** Per-Runtime focused/hovered FiberId and pointer captures, separate from scheduling state.

### Event Routing
- **Definition:** Capture → target → bubble through Fiber, using Hit Testing and InputState; applies EventCtx commands after the walk.
- **Synonyms:** Event Walk

### Button
- **Definition:** Focusable onClick widget with hover/pressed/focused states, also used by paste confirmation.
- **Relationships:**
  - implements AnyView

### Paste Confirmation Window
- **Definition:** An OS-level secondary winit window that displays the paste confirmation UI independently of Harbor's main window.
- **Relationships:**
  - communicates with Harbor Widget Runtime
  - depends on Cross-Window Input Gate

### Cross-Window Input Gate
- **Definition:** An App-owned policy that blocks terminal keyboard input and new paste requests while a Paste Confirmation Window exists.

## Terminal Domain

### Terminal
- **Definition:** The GPU-independent terminal engine/session abstraction owning screen state, VT parsing, input, selection, timing and PTY I/O. A separate wgpu renderer consumes coherent snapshots/damage; reading does not acknowledge damage, so skipped/failed draws retain pending changes. Window/Device ownership stays with the host, with GPU access injected through Terminal Widget Bridge. See [ADR 0045](./adr/0045-gpu-independent-terminal-core-boundary.md).
- **Synonyms:** Terminal Engine
- **Relationships:**
  - contains Wide Cell invariant

### Wide Cell
- **Definition:** A terminal grid representation in which a double-width glyph occupies a base cell and an adjacent continuation cell.
- **Synonyms:** Wide Character Cell, Wide Glyph

### Wide Cell Normalization
- **Definition:** Screen edits touching either half clean or preserve the whole wide glyph, respecting Protected Cell and Damage Tracking invariants.
- **Synonyms:** Wide-Cell Normalization

### Screen Editing Operation
- **Definition:** Erase, insert/delete, line movement, scroll or rectangular mutation using Wide Cell Normalization, updating Damage Tracking and pending-wrap as applicable.

### Wide Cell Invariant
- **Definition:** A screen-row consistency rule requiring every continuation cell to have its corresponding wide-glyph base and every wide-glyph base to have an adjacent continuation cell within the row boundary.

### CustomPaint
- **Definition:** External-draw escape emitting Primitive::External plus ExternalDrawId and registering a handler through BuildCx. Direct pointer focus forwards keyboard input including Tab; it is not a Tab/Shift+Tab traversal destination and Tab does not leave it.
- **Synonyms:** External Draw Widget

### Terminal Widget Bridge
- **Definition:** A Component that adapts harbor-widget rendering and input types to terminal-owned boundary types while embedding a Terminal through CustomPaint.
- **Relationships:**
  - references ExternalDrawId
  - communicates with UiEvent

### ExternalDrawFn
- **Definition:** A per-ExternalDrawId callback invoked by Runtime during scene encoding, borrowing draw context, frame-scoped GPU access and the active render pass rather than owning host resources.

### ExternalDrawId
- **Definition:** A `u64` identifier linking a `CustomPaint` widget to its `ExternalDrawFn` handler in the Runtime.
- **Synonyms:** Draw ID

### Autowrap (DECAWM)
- **Definition:** DEC private ?7: reaching the right margin sets Pending-Wrap; the next printable first wraps to the next line.
- **Synonyms:** DECAWM, auto-wrap, autowrap mode

### Pending-Wrap
- **Definition:** Cursor flag deferring autowrap until the next printable; distinct from retained row Soft-Wrap Marker and remapped by resize/editing.
- **Synonyms:** pending-wrap state, wrap pending

### IRM (Insert Mode)
- **Definition:** ANSI mode 4 (`CSI 4 h/l`): printing inserts and shifts cells right inside active Horizontal Margins, normalizing wide cells, rather than overwriting.
- **Synonyms:** Insert-Replace Mode, insert mode, IRM

### Horizontal Margins (DECLRMM)
- **Definition:** The inclusive left/right column boundaries set by DECSLRM (`CSI Pl;Pr s`) and enabled by DEC private mode 69, which bound editing, insertion, deletion, scrolling, and cursor positioning to that column range.
- **Synonyms:** Left/Right Margins, DECLRMM, DECSLRM

### Soft-Wrap Marker
- **Definition:** Per-row continuation metadata joining physical rows into a Logical Line, unlike an explicit newline; produced by printing and consumed by reflow/copy.
- **Synonyms:** soft-wrap metadata, wrapped flag, continuation row

### Logical Line
- **Definition:** The sequence of one or more physical rows forming a single application-written line, joined by soft-wrap markers and ended by an explicit newline.
- **Synonyms:** wrapped line

### Screen Resize
- **Definition:** Buffer-specific grid change: primary logical reflow versus alternate rectangular resize, with a Windows ConPTY producer-owned live-primary exception defined under Reflow. Approval and runtime acceptance are separate; see [ADR 0052](./adr/0052-conpty-live-primary-producer-geometry-and-styled-tail-clipping.md).
- **Synonyms:** resize
- **Relationships:**
  - references Soft-Wrap Marker

### Reflow
- **Definition:** Re-wraps primary Logical Lines at a new width, recomputing soft-wrap markers/projections. Full history/non-ConPTY reflow discards only ordinary default-style, unprotected, non-hyperlinked tail spaces not required by live/saved cursors; discarded tails are unavailable for widening/copy/selection ([ADR 0046](./adr/0046-trim-ordinary-trailing-spaces-during-primary-reflow.md)). Windows ConPTY live primary instead follows producer text/cursor geometry: style-only tails add no rows, attributes survive within target-row capacity, overflow is discarded without restoration/copy promises. Already-forced prefixes retain producer geometry; history/live boundaries and protected/hyperlinked content are not globally reclassified. Saved primary uses the same region policy; alternate remains rectangular ([ADR 0052](./adr/0052-conpty-live-primary-producer-geometry-and-styled-tail-clipping.md)).
- **Synonyms:** reflow on resize, resize reflow
- **Relationships:**
  - belongs to Screen Resize

### Meaningful Blank
- **Definition:** Retained logical blank content, unlike unwritten capacity or generated padding. Printed spaces and styled/hyperlinked blanks are meaningful; default-style erase leaves capacity, visibly styled erase leaves meaningful blanks. Reflow's scoped tail-loss exceptions do not make concealed source text disposable.
- **Synonyms:** content blank, retained blank
- **Relationships:**
  - belongs to Logical Line

### Logical Line Identity
- **Definition:** Monotonic non-reused ID shared by a line's physical rows, stable under reflow/character edits. Explicit newline, independent-row clearing, reset and alternate creation establish fresh identities.
- **Synonyms:** logical line ID

### Content Anchor
- **Definition:** Durable logical position: Logical Line Identity, offset and before/after affinity; physical generation/column is only a projection. Overwrite preserves offset, earlier insert/delete adjusts it by affinity, movement/reflow preserves identity, and line destruction/eviction invalidates it.
- **Synonyms:** logical anchor, stable content anchor

### Content Anchor Consumer
- **Definition:** Anything that resolves a Content Anchor and therefore needs mutation bookkeeping: a pointer selection, a saved cursor position and the viewport review position. With none present the Screen records no anchor mutation and builds no Content Projection, so ingest cost does not grow with retained history; the I/O owner registers selection interest with the Screen before parsing a feed instead of the Screen querying the pointer. [ADR 0074](./adr/0074-demand-driven-content-anchor-tracking.md)
- **Synonyms:** anchor tracking interest
- **Relationships:**
  - references Content Anchor

### Reflow Capacity Eviction
- **Definition:** After reflow, enforce physical-row capacity oldest-first; mark partial retained lines head-truncated, invalidate evicted anchors and move evicted review to oldest retained content. The selected extension adds an independent retained-history-content byte limit with the same oldest-first direction, distinct from measured process/GPU memory; [ADR 0072](./adr/0072-dual-bounded-terminal-history.md) records that direction and [ADR 0073](./adr/0073-session-history-accounting-and-text-unit-overflow.md) defines per-session UTF-8 history accounting, live reload and the exceptional oversized-unit safety policy. These records do not establish implementation.
- **Synonyms:** reflow eviction
- **Relationships:**
  - references Content Anchor
  - references Logical Line Identity

### Logical Reflow Atom
- **Definition:** Temporary shared reflow/copy unit: a complete one-/two-cell glyph with style, hyperlink and meaningful-blank state, or an explicit hard break. Continuations and generated edge padding are projection details, not independent content.
- **Synonyms:** reflow atom, logical atom stream
- **Relationships:**
  - contains Meaningful Blank
  - references Soft-Wrap Marker
  - references Wide Cell

### Primary Height Resize Policy
- **Definition:** Keeps live bottom/cursor anchored: shrink moves top live rows to history, growth pulls newest history back, and review follows its Content Anchor. Width reflow precedes height allocation. ConPTY preserves the `PreserveLiveTop` history/live boundary rather than projecting history into producer live coordinates.
- **Relationships:**
  - belongs to Screen Resize

### Minimum Terminal Width
- **Definition:** Harbor normalizes terminal, model, and PTY geometry to at least two columns so every retained width-two glyph has a valid physical projection without replacement or hidden overflow storage; row count remains at least one.

### Transactional Terminal Resize
- **Definition:** Acquire PTY Resize Barrier, parse preceding chunks under old geometry, prepare all buffers/mappings, resize PTY, then commit by allocation-free infallible swap. Bounded acquisition/preparation/PTY failures retain prior geometry, resume the reader and permit retry. See [ADR 0043](./adr/0043-acknowledged-pty-resize-barrier.md).
- **Synonyms:** prepared resize, resize transaction
- **Relationships:**
  - belongs to Screen Resize

### PTY Resize Barrier
- **Definition:** Epoch-qualified reader acknowledgement after publishing every completed pre-acknowledgement read exactly once, followed by a pause until explicit resume. Acquisition is bounded; all paths resume the reader. It orders reader-observed chunks, not child-generation time for unread OS-pipe bytes.
- **Synonyms:** acknowledged reader barrier, resize output barrier
- **Relationships:**
  - belongs to Transactional Terminal Resize

### Canonical Selection Anchor
- **Definition:** Selection endpoints durably store Content Anchors; GenPos is a current-geometry cache. Loss of either endpoint invalidates the whole selection.

### RIS
- **Definition:** The "Reset to Initial State" control sequence (`ESC c`) that performs a hard reset, clearing the screen and resetting cursor, modes, margins, scroll region, pen state, and tab stops.
- **Synonyms:** hard reset, full reset

### DECSTR
- **Definition:** The "Soft Terminal Reset" control sequence (`CSI ! p`) that resets terminal modes and pen state while leaving screen content intact.
- **Synonyms:** soft reset

### Terminal Frame Demand
- **Definition:** A host-neutral terminal scheduling output that reports an immediate redraw requirement and the next cursor-blink deadline to its rendering host.
- **Relationships:**
  - communicates with Runtime Host

### Synchronized Output Mode
- **Definition:** The DEC private mode `?2026` that batches terminal output by suppressing ordinary presentation while a saturated session-owned nesting counter is nonzero.
- **Synonyms:** synchronized output, sync mode
- **Relationships:**
  - communicates with Runtime Frame Scheduler
  - references DECRQM
  - references RIS
  - references DECSTR

### Cursor Blink Scheduling
- **Definition:** The timer-driven cursor policy that alternates visibility while idle and resets to visible when terminal input or cursor movement occurs.
- **Relationships:**
  - belongs to Terminal Frame Demand
  - depends on Runtime Frame Scheduler

### External Draw Scheduling Callback
- **Definition:** A widget Runtime callback registered by an external draw provider to contribute an immediate redraw request or animation deadline before the scheduler chooses its idle wait policy.
- **Relationships:**
  - extends ExternalDrawFn

### Standalone Terminal Winit/WGPU Adapter
- **Definition:** A feature-gated public host adapter that renders Terminal directly through winit and wgpu without creating a Harbor Widget Runtime.
- **Relationships:**
  - communicates with Runtime Host

### Windows Acrylic Backdrop
- **Definition:** Host/compositor material behind the main window, independent of terminal palette and tint. Unavailable backdrop makes window base and terminal background opaque.
- **Synonyms:** system acrylic, DWM system backdrop, DesktopAcrylicController backdrop, AccentPolicy backdrop, Windows Terminal-style acrylic
- **Relationships:**
  - belongs to Runtime Host
  - depends on Dual Composition Target Stack
  - depends on Default Background Cell
  - contains System Caption Chrome

### System Caption Chrome
- **Definition:** The DWM-drawn minimize, maximize, and close buttons on the Harbor main window, with caption text and icon left undrawn so acrylic fills the title strip.
- **Synonyms:** system caption buttons, non-client buttons
- **Relationships:**
  - belongs to Runtime Host

### Windows 10 Caption Degradation
- **Definition:** An accepted Windows 10 limitation where the system caption strip may remain opaque on some themes while the client area is still acrylic.
- **Relationships:**
  - belongs to Windows Acrylic Backdrop

### Inverse Default Cell
- **Definition:** An inverse cell whose foreground and background are both `Color::Default`, filled with the active default foreground and glyph-painted with the active default background RGB. Startup configuration and supported OSC default-color changes supply that palette.

### Default Background Cell
- **Definition:** A terminal cell whose background is `Color::Default`, revealing the terminal background layer's active palette color. Its alpha is preserved over an available backdrop and forced opaque otherwise; startup TOML and supported OSC default-color sequences determine the color, not a fixed warm-brown constant.

### Terminal Window Inset
- **Definition:** An application-level layout rule that wraps the main workspace (side rail and terminal panel) in uniform 4 logical-pixel padding. The terminal panel's own decoration and grid padding are separate.
- **Relationships:**
  - depends on Padding Widget
  - belongs to Runtime Host

### Terminal Decoration Preset
- **Definition:** The current application-level terminal appearance uses an 8dp radius, a 10%-opaque black outer shadow with 1dp downward offset and 1dp blur, zero spread, no default fill, and anti-aliased child clipping. Earlier specs retain their original preset as historical design intent.
- **Relationships:**
  - contains BoxDecoration
  - references ClipBehavior
  - belongs to Runtime Host

### External Frame Appearance
- **Definition:** A former per-runtime provider for the whole-surface clear color, removed in favor of a transparent frame clear for external-draw runtimes and host-owned base fills.

### Window Base Fill
- **Definition:** An opaque dark-gray fill owned by the root Padding element painted only when no compositor backdrop is available, tinting the window inset and rounded-corner cutouts in one paint path.
- **Relationships:**
  - belongs to Runtime Host

### Window Backdrop Tint
- **Definition:** Host compositor style, not terminal palette: WASDK/AccentPolicy use WindowBackdropStyle (default white, 0.06 opacity); DWM TransientWindow uses system material, then opaque base fallback.
- **Relationships:**
  - belongs to Runtime Host
  - implements Windows Acrylic Backdrop

### Window Backdrop Backend
- **Definition:** Bootstrap host abstraction over WASDK DesktopAcrylicController, DWM TransientWindow, AccentPolicy and opaque fallback; exports no platform resources or sampleable desktop visual to Terminal.
- **Relationships:**
  - belongs to Runtime Host
  - implements Window Backdrop Tint
  - depends on Dual Composition Target Stack

### Dual Composition Target Stack
- **Definition:** WASDK-only same-HWND stack: lower topmost=false DesktopWindowTarget hosts acrylic; separate upper topmost=true DirectComposition target hosts transparent wgpu. Other backends need not use it.
- **Synonyms:** dual-target composition, two-layer composition target stack
- **Relationships:**
  - belongs to Runtime Host
  - implements Windows Acrylic Backdrop

### Rounded Clip Mask
- **Definition:** Dest-in GPU coverage mask multiplying framebuffer RGB/alpha to remove external pixels outside ancestor rounded clips, revealing host/frame clear.
- **Relationships:**
  - belongs to Widget Renderer
  - implements ClipBehavior

### Manual Configuration Reload
- **Definition:** Application-command transaction validating a complete TOML candidate and preparing resources before coherent publication to applicable existing sessions and future factories. Rejection retains accepted settings, never startup fallback; it is not widget HMR. Fonts use Terminal Font Policy; OSC overrides use the latest accepted reset baseline. See [ADR 0059](./adr/0059-atomic-manual-reload-and-terminal-only-fonts.md) and [ADR 0060](./adr/0060-font-reload-uses-normal-bounded-resize.md).
- **Failure qualification:** A failed compensating native PTY resize permits an explicitly reported recovery-failed outcome, with old accepted settings retained, all readers resumed, geometry reconciliation attempted, and unrecovered sessions identified. Ordinary failures do not permit partial acceptance. See [ADR 0061](./adr/0061-manual-reload-compensation-failure.md).
- **Relationships:**
  - belongs to Application Business Host
  - references Application Command Dispatcher
  - references OSC Default Color Policy
  - references OSC Indexed Palette Policy

### Watched Configuration Reload
- **Definition:** Default-on, debounced file-save/replacement trigger for the same strict Manual Configuration Reload transaction. Invalid edits retain accepted state, identical failures avoid flooding, watcher faults stay visible and manual reload remains available. See [ADR 0062](./adr/0062-watched-reload-and-new-session-launch-settings.md).
- **Relationships:**
  - belongs to Application Business Host

### New-Session Launch Settings
- **Definition:** Shell program/arguments, environment overlays and initial directory for future children only; reload changes neither running children nor Harbor's global environment/directory. Values are literal, TERM is protected, relative paths resolve against the configuration file, and unavailable configured directories fail rather than fall back. Validation, inheritance and confidentiality: [ADR 0062](./adr/0062-watched-reload-and-new-session-launch-settings.md).
- **Relationships:**
  - references TOML User Settings
  - references Manual Configuration Reload

### TOML User Settings
- **Definition:** User preferences from `~/.harbor/config.toml`, except Portable Distribution Mode. Startup uses defaults for missing files, independent scalar fallback, and whole-palette fallback for invalid colors; live reload instead uses strict complete-candidate acceptance.

### Portable Distribution Mode
- **Definition:** Windows ZIP mode selected by `harbor.portable` beside `harbor.exe`: uses adjacent `config.toml`, never reads/writes home configuration or creates/overwrites settings. Installed builds retain the home path. See [ADR 0057](./adr/0057-windows-installation-and-portable-isolation.md).
- **Synonyms:** full portable mode, portable ZIP
- **Relationships:**
  - references TOML User Settings

### Widget Store
- **Definition:** UI-thread Signal state plus private FIFO action inbox. Host publishes state and owns reducers/effects; Store does neither reduction nor automatic Runtime wakeup.
- **Relationships:**
  - communicates with Runtime Host
  - implements Event-Turn Action Transport

### Dispatcher
- **Definition:** Cloneable write-only Widget Store capability enqueueing typed intent FIFO; cannot read state, reduce, execute effects or wake Runtime.

### Event-Turn Action Transport
- **Definition:** Widgets enqueue one-shot FIFO intent during routing; after widget effects, Application Business Host drains/reduces it and owns models/platform effects.

### Application Command
- **Definition:** User-invokable operation with stable external string ID and typed internal form, unlike PTY output, redraw and window lifecycle notifications.
- **Relationships:**
  - belongs to Application Business Host

### Application Command Registry
- **Definition:** Application catalog of implemented command IDs, display metadata, default bindings and availability, shared by shortcuts and future palette.
- **Relationships:**
  - belongs to Application Business Host

### Application Command Dispatcher
- **Definition:** Single command path for shortcuts/buttons/menus/future palette. Shortcuts synchronously return Command Outcome; only consumed commands enter scoped FIFO for later Host-owned execution. Not a pub/sub bus.

### Command Palette
- **Definition:** Application-owned searchable presentation of implemented, palette-visible Application Commands through the existing registry and dispatcher. Current unavailability visibly disables an action without displaying a reason rather than forwarding palette input to the PTY; execution revalidates availability and respects the resulting action's focus destination. See [ADR 0069](./adr/0069-command-palette-source-aware-availability-and-focus.md) and its reason-display override in [ADR 0078](./adr/0078-command-palette-disabled-items-without-reasons.md).
- **Relationships:**
  - belongs to Application UI Composition
  - depends on Application Command Registry
  - communicates with Application Command Dispatcher
  - references Widget Text Input

### Application Keybindings
- **Definition:** Structured TOML command tables with `bindings` records carrying explicit modifiers and key. Omission retains defaults; an empty array unbinds. Matched available commands get first refusal before terminal input. Invalid overrides, including the superseded string-array format, reject the entire override set without discarding other valid startup settings. See [ADR 0041](./adr/0041-structured-keybinding-configuration.md).
- **Relationships:**
  - communicates with Terminal Input Semantics

### Command Outcome
- **Definition:** Consumed enqueues a command and blocks terminal delivery; PassThrough queues nothing and forwards the same event. Copy-or-interrupt passes through without selection; copy always consumes, including empty copy.
- **Relationships:**
  - communicates with Terminal Selection Copy Policy

### Terminal Tab
- **Definition:** An application-owned terminal session identified by a stable TabId, retaining its Terminal and PTY resources independently of whether its side-rail item is active.
- **Relationships:**
  - communicates with Terminal Widget Bridge

### Terminal Title Policy
- **Definition:** Per-tab shell-executable fallback, replaced by valid OSC 0/1/2 UTF-8 titles (no controls, at most 256 Unicode characters); empty/reset restores fallback and invalid input preserves the title. All tab labels update independently; only the active tab sets `<title> — Harbor`. OSC 7 directory metadata is separate, never inferred from titles/startup.
- **Relationships:**
  - belongs to Terminal Tab
  - communicates with Application Business Host
  - references TOML User Settings

### Terminal Tab Manager
- **Definition:** The Application Business Host model that owns ordered Terminal Tabs, selects one active tab for input and presentation, and routes tab-qualified background output events.
- **Relationships:**
  - communicates with Harbor Widget Runtime

### Widget View Macro
- **Definition:** Experimental Rust-expression `view!` syntax: semicolon child values through `IntoChildView`, `expression => { children }` components and child-list `for`/`if`/`match`; no context parameter. Every composition position — root, child and content slot — supplies the same deferred description; the root Fiber is allocated by the Runtime at the reconciliation top level. See [ADR 0032](./adr/0032-view-macro-rust-expression-grammar.md) for the grammar and [ADR 0079](./adr/0079-defer-root-construction-and-unify-content-slot-boundaries.md) for the construction boundary.
- **Synonyms:** view! DSL
- **Relationships:**
  - belongs to Harbor Widget Runtime

### Deferred Root
- **Definition:** The `view!` macro root supplied as the same deferred component description child positions use; the Runtime allocates the root Fiber at the reconciliation top level and builds the root component with its own Fiber-owned BuildCx. A build return that defers another component flattens in place on that Fiber within a bounded depth. See [ADR 0079](./adr/0079-defer-root-construction-and-unify-content-slot-boundaries.md).
- **Synonyms:** Root Deferral
- **Relationships:**
  - belongs to Harbor Widget Runtime
  - references Fiber
  - references Reconciliation

### Hex Color Setting
- **Definition:** A TOML color string in `#RRGGBB` or `#RRGGBBAA` form, where omitted alpha means opaque and eight-digit colors preserve alpha for terminal backgrounds.
- **Relationships:**
  - belongs to TOML User Settings

### Terminal Theme
- **Definition:** Terminal-only color scheme, not application chrome or its day/night mode; theme selection must not recolor rail/buttons. This boundary does not establish a theme UI; see [ADR 0058](./adr/0058-terminal-themes-separate-from-application-appearance.md).
- **Synonyms:** Terminal color scheme

### Application Appearance Mode
- **Definition:** Application day/night (light/dark) policy, independent of Terminal Theme and compositor tint. Only two overall modes are approved; defaults, system following, controls and persistence remain unspecified. See [ADR 0058](./adr/0058-terminal-themes-separate-from-application-appearance.md).
- **Synonyms:** Day mode, Night mode, Light mode, Dark mode
- **Relationships:**
  - belongs to Application Business Host

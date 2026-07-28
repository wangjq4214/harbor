# Context

Project domain concepts and terminology.

### Harbor Widget Runtime
- **Definition:** A declarative GPU UI runtime based on Rust and wgpu, managing the full UI pipeline from component state changes to retained GPU scene encoding.
- **Synonyms:** Widget Runtime, Runtime
- **Relationships:**
  - contains Fiber
  - contains RenderNode
  - contains Signal
  - contains View
  - communicates with wgpu Renderer

### View
- **Definition:** An immutable UI description produced by component build methods, discardable after each build pass.
- **Synonyms:** Widget
- **Relationships:**
  - consumed by Reconciler
  - produced by Component

### Fiber
- **Definition:** A long-lived node that retains component identity, hook/state, Signal subscriptions, child fibers, and dirty flags across rebuilds.
- **Synonyms:** Fiber Node
- **Relationships:**
  - references RenderNode
  - subscribes to Signal
  - belongs to Harbor Widget Runtime

### RenderNode
- **Definition:** A long-lived node holding layout results, transforms, clipping, paint order, and hit-testable regions.
- **Relationships:**
  - referenced by Fiber
  - consumed by Retained Scene Graph

### Signal
- **Definition:** A fine-grained, pull-based reactive state source on the UI thread; writes mark subscribed fibers dirty via dirty-flag model.
- **Synonyms:** State Cell
- **Relationships:**
  - subscribed by Fiber

### Key
- **Definition:** A stable identity marker used during reconciliation to match views between build passes and preserve fiber state.
- **Relationships:**
  - used by Reconciler
  - belongs to View

### Reconciliation
- **Definition:** The process of diffing the new View tree against the existing Fiber tree, reusing fibers where type and Key match, and destroying mismatched subtrees.
- **Synonyms:** Reconciler
- **Relationships:**
  - consumes View
  - produces Fiber Tree

### BoxConstraints
- **Definition:** A layout primitive expressing the minimum and maximum size a parent imposes on a child, driving the single-pass layout algorithm.
- **Relationships:**
  - consumed by RenderNode layout

### Generation Arena
- **Definition:** A slotmap-based array where each slot has a generation counter; stale references (FiberId) are detected by generation mismatch on access.
- **Synonyms:** SlotMap
- **Relationships:**
  - implements FiberId safety

### Component
- **Definition:** A trait with a single `build` method that takes `&mut BuildCx` and returns an immutable View; it owns no lifecycle state itself.
- **Synonyms:** Widget Builder
- **Relationships:**
  - produces View
  - referenced by Runtime

### BuildCx
- **Definition:** The build context passed to Component::build, providing `use_state` for hook/signal creation and transparent Signal dependency tracking.
- **Relationships:**
  - creates Signal
  - belongs to Reconciliation

### AnyView
- **Definition:** An internal trait that enables type-erased View storage, exposing `key()` for reconciliation matching and `build()` to produce the next View.
- **Relationships:**
  - wrapped by View

### Hook
- **Definition:** A per-Fiber slot storing Signal state across rebuilds; `use_state` looks up the current fiber's hook list by call order via `BuildCx`'s direct fiber reference.
- **Relationships:**
  - stored in Fiber
  - accessed by BuildCx

### Primitive
- **Definition:** A standardized draw input produced by RenderNode, describing a single GPU draw call: Quad (colored rect with optional corner radius), Text, Border, or External delegate.
- **Relationships:**
  - produced by RenderNode
  - consumed by Scene Graph

### SceneItem
- **Definition:** A retained GPU-visible draw item in the scene graph with a Primitive, local transform, clip region, and paint order index.
- **Relationships:**
  - contains Primitive
  - belongs to Scene Graph

### SceneDelta
- **Definition:** An incremental update describing added, removed, or modified SceneItems since the last frame; consumed by the widget renderer to update GPU buffers.
- **Relationships:**
  - references SceneItem
  - consumed by Widget Renderer

### Scene Graph
- **Definition:** A retained flat ordered list of SceneItems sorted by paint order, enabling incremental GPU updates without rebuilding vertex buffers every frame.
- **Synonyms:** Retained Scene
- **Relationships:**
  - contains SceneItem
  - consumes Primitive
  - belongs to Harbor Widget Runtime

### Widget Renderer
- **Definition:** The wgpu-based instanced quad renderer inside harbor-widget that owns its own pipelines, vertex/index buffers, and processes SceneDelta to encode draw calls into a shared RenderPass.
- **Relationships:**
  - consumes SceneDelta
  - uses wgpu
  - belongs to Harbor Widget Runtime

### Layout Container
- **Definition:** A widget that positions child widgets according to a layout algorithm: Padding (inset), Row (horizontal flex), Column (vertical flex), Stack (overlay), Align (position within parent).
- **Synonyms:** Container Widget
- **Relationships:**
  - produces View
  - implemented by AnyView

### Viewport
- **Definition:** The logical pixel size, physical pixel size, and scale factor passed to Runtime::encode for converting dp layout coordinates to GPU NDC.
- **Relationships:**
  - consumed by Widget Renderer
  - provided by Host

### UiEvent
- **Definition:** A single enum representing all input events (pointer, keyboard, focus) dispatched through the widget tree via capture-target-bubble routing.
- **Synonyms:** Event
- **Relationships:**
  - consumed by Runtime::dispatch
  - handled by AnyView::handle_event

### EventCtx
- **Definition:** A command buffer passed to event handlers; supports request_focus, capture_pointer, release_pointer, invalidate_paint, and stop_propagation. Commands are applied after the event walk completes.
- **Relationships:**
  - produced by Runtime event routing
  - consumed by AnyView::handle_event

### FocusScope
- **Definition:** A widget that wraps a subtree and manages Tab/Shift+Tab focus traversal within it; supports a modal flag that blocks events from reaching widgets outside the scope.
- **Relationships:**
  - implements AnyView
  - manages FocusNode ordering

### Hit Testing
- **Definition:** Reverse-paint-order traversal of the Render Tree checking point-in-rect per widget, used to determine the event target for pointer events.
- **Relationships:**
  - uses RenderNode layout rects
  - invoked by event routing

### Pointer Capture
- **Definition:** A mechanism where a widget that receives a pointer-down can request to receive all subsequent move/up/cancel events for that pointer, even if the pointer moves outside its bounds.
- **Relationships:**
  - managed by InputState
  - requested via EventCtx::capture_pointer

### InputState
- **Definition:** A per-Runtime struct holding focused FiberId, hovered FiberId, and pointer capture map; extracted from Runtime to keep it focused on scheduling.
- **Relationships:**
  - belongs to Runtime
  - consumed by event routing

### Event Routing
- **Definition:** The capture → target → bubble walk through the Fiber tree, where hit testing identifies the target, then handlers are called in three phases before EventCtx commands are applied.
- **Synonyms:** Event Walk
- **Relationships:**
  - consumes UiEvent
  - uses InputState
  - produces EventCtx commands

### Button
- **Definition:** A focusable widget with an onClick callback and hover/pressed/focused visual states; used for paste confirmation buttons and other clickable UI in Phase 2.
- **Relationships:**
  - implements AnyView
  - handles Pointer and Focus events

### Paste Confirmation Window
- **Definition:** An OS-level secondary winit window that displays the paste confirmation UI independently of Harbor's main window.
- **Relationships:**
  - communicates with Harbor Widget Runtime
  - depends on Cross-Window Input Gate

### Cross-Window Input Gate
- **Definition:** An App-owned policy that blocks terminal keyboard input and new paste requests while a Paste Confirmation Window exists.
- **Relationships:**
  - belongs to Paste Confirmation Window

## Terminal Domain

### Terminal
- **Definition:** A self-contained engine owning terminal screen state, VT parsing, wgpu-based rendering (text, cursor, background, scrollbar, selection), and PTY I/O. It does NOT own GpuContext, a window, or a wgpu Device — those are injected by CustomPaint at render time.
- **Synonyms:** Terminal Engine
- **Relationships:**
  - contains Screen state
  - contains Parser (from harbor-parser)
  - wrapped by CustomPaint
  - depends on wgpu, harbor-parser, harbor-text

### CustomPaint
- **Definition:** A widget that explicitly marks an "escape hatch" from the normal widget rendering path. It produces a `Primitive::External` with an `ExternalDrawId`, delegating actual rendering to an externally registered handler (e.g., Terminal). During `build()`, it registers its draw handler into `BuildCx` so the Runtime can call it during encode.
- **Synonyms:** External Draw Widget
- **Relationships:**
  - produces Primitive::External
  - registers ExternalDrawFn into Runtime via BuildCx
  - wraps Terminal
  - receives GpuContext externally for injection

### ExternalDrawFn
- **Definition:** A callback type `dyn Fn(ExternalDrawId, Rect, &mut RenderPass)` registered per `ExternalDrawId`, invoked by Runtime during encode for each `Primitive::External` encountered.
- **Relationships:**
  - stored in Runtime HashMap
  - invoked by Runtime::encode

### ExternalDrawId
- **Definition:** A `u64` identifier linking a `CustomPaint` widget to its `ExternalDrawFn` handler in the Runtime.
- **Synonyms:** Draw ID
- **Relationships:**
  - stored in CustomPaint
  - key in Runtime handler map

## Parser Domain

### harbor-parser
- **Definition:** A zero-dependency streaming VT/ANSI byte state machine crate. It exposes exactly three public items: `VtHandler` trait, `Params`, and `Parser`. All internal state (accumulator, UTF-8 decoder) is private.
- **Synonyms:** VT Parser
- **Relationships:**
  - depends on nothing
  - consumed by harbor-terminal

### VtHandler
- **Definition:** A trait with 9 callback methods (`print`, `execute`, `csi_dispatch`, `esc_dispatch`, `osc_dispatch`, `dcs_hook`, `dcs_put`, `dcs_unhook`, `string_start`) that receives fully-parsed VT actions from the Parser. Static dispatch via generics.
- **Relationships:**
  - implemented by harbor-terminal Screen handler
  - called by Parser::advance

### Params
- **Definition:** An opaque container for CSI/DCS numeric parameters, supporting flat iteration and colon-separated sub-parameters. Internal representation is hidden.
- **Relationships:**
  - passed to VtHandler::csi_dispatch and VtHandler::dcs_hook
  - produced by Parser

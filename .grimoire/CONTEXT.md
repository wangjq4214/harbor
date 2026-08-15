# Context

Project domain concepts and terminology.

### System-Native Font Loading
- **Definition:** A `harbor-text` strategy in which system font discovery and font data loading use operating-system APIs to avoid copying complete font files into the Rust heap.

### System Default Font Selection
- **Definition:** The Windows `harbor-text` policy that honors an environment-variable font override and otherwise delegates primary font selection to the operating system.
- **Relationships:**
  - depends on System-Native Font Loading

### DirectWrite Font Backend
- **Definition:** The Windows `harbor-text` backend that uses DirectWrite for font selection, system fallback, metrics, glyph resolution, and rasterization while retaining the existing WGPU glyph atlas.
- **Relationships:**
  - implements System-Native Font Loading

### System Font Fallback
- **Definition:** The DirectWrite-based policy that resolves missing characters through Windows fallback fonts for both environment-overridden and system-selected primary fonts.
- **Relationships:**
  - belongs to DirectWrite Font Backend

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

### Runtime Host
- **Definition:** The binary-layer owner of application entry, winit window and surface lifetimes, GPU device resources, fatal-error policy, and cross-window coordination.
- **Synonyms:** Host, App
- **Relationships:**
  - communicates with Harbor Widget Runtime
  - contains Winit Event Adapter
  - provides WinitFrameTarget

### Winit Event Adapter
- **Definition:** A feature-gated harbor-widget adapter that converts winit input and lifecycle events into platform-independent runtime input without exposing winit types to the core Runtime.
- **Relationships:**
  - belongs to Runtime Host
  - produces UiEvent
  - communicates with Harbor Widget Runtime

### WinitFrameTarget
- **Definition:** A frame-scoped borrowed bundle of Window, Surface, Device, and Queue references that lets the winit runtime integration render and present without retaining platform resources.
- **Relationships:**
  - provided by Runtime Host
  - consumed by Harbor Widget Runtime

### Runtime Frame Presentation
- **Definition:** The runtime-owned frame policy that acquires the current SurfaceTexture, encodes and submits GPU work, notifies the window, and presents the completed frame using a borrowed WinitFrameTarget.
- **Relationships:**
  - belongs to Harbor Widget Runtime
  - depends on WinitFrameTarget
  - extends Widget Renderer

### Runtime Integration Boundary
- **Definition:** The public feature-gated harbor-widget API that handles generic window events and frame presentation while excluding terminal, paste, and other Harbor-specific business policy.
- **Relationships:**
  - contains Winit Event Adapter
  - contains Runtime Frame Presentation
  - communicates with Runtime Host

### External Runtime Invalidation
- **Definition:** A platform-independent signal through which Host-owned asynchronous sources mark a Runtime dirty without exposing their business-specific event types to harbor-widget.
- **Relationships:**
  - consumed by Harbor Widget Runtime
  - produces RuntimeEffects

### TerminalOutputReady
- **Definition:** A Host-owned application event emitted when the PTY reader has terminal output ready for UI-thread processing.
- **Relationships:**
  - produces External Runtime Invalidation
  - belongs to Runtime Host

### Runtime Frame Scheduler
- **Definition:** The runtime-owned state machine that converts invalidation, animation deadlines, and steady-state activity into Wait, WaitUntil, Poll, and RequestRedraw effects.
- **Relationships:**
  - belongs to Harbor Widget Runtime
  - consumes External Runtime Invalidation
  - produces RuntimeEffects

### Terminal Input Semantics
- **Definition:** The harbor-terminal responsibility for interpreting routed platform-independent keyboard and pointer events as terminal actions such as scrollback navigation.
- **Relationships:**
  - belongs to Terminal
  - consumes UiEvent

### RuntimeEffects
- **Definition:** A platform-independent command batch returned by Runtime for its Host to apply, including redraw, cursor, IME, and clipboard requests.
- **Synonyms:** Runtime Effects
- **Relationships:**
  - produced by Harbor Widget Runtime
  - consumed by Runtime Host

### Runtime Per Window
- **Definition:** The ownership model in which each OS window has an independent Runtime, Widget tree, and input state while GPU and text resources may be shared.
- **Relationships:**
  - depends on Runtime Host
  - contains Harbor Widget Runtime

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
- **Definition:** A self-contained engine owning terminal screen state, VT parsing, wgpu-based rendering (text, cursor, background, scrollbar, selection), and PTY I/O. It does NOT own GpuContext, a window, or a wgpu Device — those are injected by Terminal Widget Bridge at render time.
- **Synonyms:** Terminal Engine
- **Relationships:**
  - contains Screen state
  - contains Wide Cell invariant
  - contains Parser (from harbor-parser)
  - wrapped by Terminal Widget Bridge
  - depends on wgpu, harbor-parser, harbor-text

### Wide Cell
- **Definition:** A terminal grid representation in which a double-width glyph occupies a base cell and an adjacent continuation cell.
- **Synonyms:** Wide Character Cell, Wide Glyph
- **Relationships:**
  - belongs to Screen state
  - normalized by Wide Cell Normalization

### Wide Cell Normalization
- **Definition:** The shared screen-editing rule that cleans or preserves a complete wide glyph whenever an operation touches either half of it.
- **Synonyms:** Wide-Cell Normalization
- **Relationships:**
  - applies to Screen Editing Operation
  - preserves Damage Tracking
  - respects Protected Cell
  - implemented by VtEditEngine helpers

### Screen Editing Operation
- **Definition:** A terminal screen mutation such as erase, character insertion or deletion, line movement, scrolling, or DEC rectangular editing.
- **Relationships:**
  - uses Wide Cell Normalization
  - produces Damage Tracking updates
  - references Pending-Wrap

### Wide Cell Invariant
- **Definition:** A screen-row consistency rule requiring every continuation cell to have its corresponding wide-glyph base and every wide-glyph base to have an adjacent continuation cell within the row boundary.
- **Relationships:**
  - validated by Screen Tests
  - maintained by Wide Cell Normalization

### CustomPaint
- **Definition:** A widget that explicitly marks an "escape hatch" from the normal widget rendering path. It produces a `Primitive::External` with an `ExternalDrawId`, delegating actual rendering to an externally registered handler (e.g., Terminal). During `build()`, it registers its draw handler into `BuildCx` so the Runtime can call it during encode.
- **Synonyms:** External Draw Widget
- **Relationships:**
  - produces Primitive::External
  - registers ExternalDrawFn into Runtime via BuildCx
  - contained by Terminal Widget Bridge
  - receives GpuContext externally for injection

### Terminal Widget Bridge
- **Definition:** A Component that adapts harbor-widget rendering and input types to terminal-owned boundary types while embedding a Terminal through CustomPaint.
- **Relationships:**
  - wraps Terminal
  - contains CustomPaint
  - references ExternalDrawId
  - communicates with UiEvent

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

### Autowrap (DECAWM)
- **Definition:** The terminal mode (DEC private mode ?7) that, when enabled, causes a printable character written at the right margin to wrap the cursor to the first column of the next line.
- **Synonyms:** DECAWM, auto-wrap, autowrap mode
- **Relationships:**
  - belongs to Terminal Modes
  - controls Pending-Wrap

### Pending-Wrap
- **Definition:** A cursor-state flag set when the cursor reaches the right margin with autowrap enabled, so that the next printable character first moves to the next line before printing.
- **Synonyms:** pending-wrap state, wrap pending
- **Relationships:**
  - controlled by Autowrap (DECAWM)
  - stored in Cursor
  - set by Print Path
  - references Soft-Wrap Marker
  - references Screen Resize
  - references Screen Editing Operation

### IRM (Insert Mode)
- **Definition:** The ANSI standard mode 4 (`CSI 4 h/l`) that makes a printable character insert a cell at the cursor and shift the remaining cells of the active horizontal area right before writing, instead of overwriting the cell under the cursor.
- **Synonyms:** Insert-Replace Mode, insert mode, IRM
- **Relationships:**
  - belongs to Terminal Modes
  - uses Wide Cell Normalization
  - constrains Screen Editing Operation
  - bounded by Horizontal Margins (DECLRMM)

### Horizontal Margins (DECLRMM)
- **Definition:** The inclusive left/right column boundaries set by DECSLRM (`CSI Pl;Pr s`) and enabled by DEC private mode 69, which bound editing, insertion, deletion, scrolling, and cursor positioning to that column range.
- **Synonyms:** Left/Right Margins, DECLRMM, DECSLRM
- **Relationships:**
  - belongs to Terminal Modes
  - bounds IRM (Insert Mode)
  - bounds Screen Editing Operation

### Soft-Wrap Marker
- **Definition:** Per-row metadata on a screen or scrollback row marking it as the continuation of a logical line that wrapped at the right margin rather than ending with an explicit newline.
- **Synonyms:** soft-wrap metadata, wrapped flag, continuation row
- **Relationships:**
  - belongs to Screen state
  - set by Print Path
  - consumed by Reflow
  - distinguishes a wrapped row from an explicit newline
  - references Pending-Wrap

### Logical Line
- **Definition:** The sequence of one or more physical rows forming a single application-written line, joined by soft-wrap markers and ended by an explicit newline.
- **Synonyms:** wrapped line
- **Relationships:**
  - contains Soft-Wrap Marker

### Screen Resize
- **Definition:** A change to the terminal grid size that copies existing rows in place without reflow.
- **Synonyms:** resize
- **Relationships:**
  - references Soft-Wrap Marker
  - references Pending-Wrap
  - references Reflow

### Reflow
- **Definition:** The resize policy that re-wraps logical lines to the new terminal width, recomputing soft-wrap markers and column positions instead of leaving rows at their pre-resize layout.
- **Synonyms:** reflow on resize, resize reflow
- **Relationships:**
  - consumes Soft-Wrap Marker
  - belongs to Screen Resize

### RIS
- **Definition:** The "Reset to Initial State" control sequence (`ESC c`) that performs a hard reset, clearing the screen and resetting cursor, modes, margins, scroll region, pen state, and tab stops.
- **Synonyms:** hard reset, full reset
- **Relationships:**
  - belongs to Reset Paths
  - resets Pending-Wrap
  - clears Soft-Wrap Marker

### DECSTR
- **Definition:** The "Soft Terminal Reset" control sequence (`CSI ! p`) that resets terminal modes and pen state while leaving screen content intact.
- **Synonyms:** soft reset
- **Relationships:**
  - belongs to Reset Paths
  - resets Pending-Wrap

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
  - references CSI Private Marker

### CSI Private Marker
- **Definition:** An optional raw byte passed through `VtHandler::csi_dispatch` to preserve the distinct `?`, `>`, `<`, and `=` CSI prefixes without adding another public parser type.
- **Relationships:**
  - belongs to harbor-parser

### Params
- **Definition:** An opaque container for CSI/DCS numeric parameters, supporting flat iteration and colon-separated sub-parameters. Internal representation is hidden.
- **Relationships:**
  - passed to VtHandler::csi_dispatch and VtHandler::dcs_hook
  - produced by Parser

### TerminalReply
- **Definition:** A platform-neutral 1024-byte buffer in the Screen struct that atomically accepts or drops complete outgoing terminal protocol replies generated by the VT parser.
- **Synonyms:** ReplySink, Screen-buffered replies
- **Relationships:**
  - belongs to Screen
  - communicates with Primary DA
  - communicates with Secondary DA
  - communicates with DSR
  - communicates with CPR
  - communicates with DECRQSS
  - communicates with XTGETTCAP

### DECRQSS
- **Definition:** A VT status-query protocol that Harbor accepts as 7-bit `DCS $ q Pt ST` and answers through TerminalReply for SGR, DECSTBM, DECSLRM, DECSCUSR, and DECSCA state.
- **Synonyms:** Request Status String
- **Relationships:**
  - depends on TerminalReply
  - references Screen state

### XTGETTCAP
- **Definition:** A VT terminfo-capability query where an application sends `DCS + q Pt ST` with `Pt` as semicolon-separated hex-encoded capability names, and Harbor replies `DCS 1 + r Pt ST` listing `name=value` pairs for its registry-backed capabilities (`TN` = `xterm-256color`, `RGB` = `8/8/8`, `u8` = UTF-8 boolean) or `DCS 0 + r ST` when none match.
- **Synonyms:** Terminal Capability Query, tcap query
- **Relationships:**
  - depends on TerminalReply
  - references Terminfo Capability Declaration

### Terminfo Capability Declaration
- **Definition:** The private compile-time list in `harbor-terminal::parser::xtgettcap` mapping supported terminfo capability names (`TN`, `RGB`, `u8`) to their XTGETTCAP reply values.
- **Relationships:**
  - referenced by XTGETTCAP

### CPR
- **Definition:** A VT control sequence reply containing the current line and column coordinates of the terminal cursor.
- **Synonyms:** Cursor Position Report
- **Relationships:**
  - depends on TerminalReply

### DSR
- **Definition:** A VT query and corresponding reply sequence used to report the operating status of the terminal.
- **Synonyms:** Device Status Report
- **Relationships:**
  - depends on TerminalReply

### Capability Registry
- **Definition:** A private compile-time declaration in `harbor-terminal::parser::device_attributes` containing Primary model `62`, capability codes `[6, 17, 22, 28]`, and Secondary identity `(1, 1, 0)` for deterministic DA reply generation.
- **Relationships:**
  - references TerminalReply
  - implements Primary DA
  - implements Secondary DA

### Primary DA
- **Definition:** The exact VT Device Attributes reply `ESC [ ? 62 ; 6 ; 17 ; 22 ; 28 c` sent for an omitted or zero DA parameter to identify Harbor conservatively as VT220/Level 2 with only evidence-backed capabilities.
- **Synonyms:** Primary Device Attributes
- **Relationships:**
  - depends on Capability Registry
  - depends on TerminalReply
  - extends DECID

### Secondary DA
- **Definition:** The stable VT Device Attributes identity reply `ESC [ > 1 ; 1 ; 0 c`, identifying VT220 compatibility, Harbor DA protocol revision 1, and no DEC hardware option.
- **Synonyms:** Secondary Device Attributes
- **Relationships:**
  - depends on Capability Registry
  - depends on TerminalReply

### Tertiary DA
- **Definition:** The unsupported VT Device Attributes query `CSI = c`, which Harbor safely ignores without producing a reply or side effect.
- **Synonyms:** Tertiary Device Attributes
- **Relationships:**
  - references TerminalReply

### DECID
- **Definition:** The legacy 7-bit identify query `ESC Z` that uses the same response builder and exact response bytes as Primary DA.
- **Synonyms:** DEC Identify
- **Relationships:**
  - depends on Primary DA
  - depends on TerminalReply

### DA Query Compatibility
- **Definition:** Harbor responds only to 7-bit DA requests whose parameter is omitted or zero, ignores invalid parameter forms, and does not recognize 8-bit C1 DA forms in this feature scope.
- **Relationships:**
  - references Primary DA
  - references Secondary DA
  - references Tertiary DA
  - references DECID

### DECRQM
- **Definition:** A VT request for the state of one standard or DEC private terminal mode, encoded as `CSI Ps $ p` or `CSI ? Ps $ p`.
- **Synonyms:** DEC Request Mode
- **Relationships:**
  - depends on TerminalReply
  - communicates with DECRPM

### DECRPM
- **Definition:** A VT response reporting a queried standard or DEC private mode as set, reset, permanently set, permanently reset, or unknown.
- **Synonyms:** DEC Report Mode
- **Relationships:**
  - references DECRQM
  - implements TerminalReply

### Streaming Parser Safety
- **Definition:** Roadmap P1 proof that the incremental parser stays bounded, cancellable, and recoverable for arbitrary byte input.
- **Synonyms:** Bounded parser contract, P1 parser safety
- **Relationships:**
  - belongs to harbor-parser
  - depends on Parser Retention Limits
  - depends on Arbitrary-Input Safety Evidence

### Parser Retention Limits
- **Definition:** Fixed caps in harbor-parser (`MAX_PARAMS` 16, `MAX_SUBPARAMS` 8, `MAX_INTERMEDIATES` 2, `MAX_CSI_PARAM` 65535, `MAX_OSC_BYTES` 4096, `MAX_STRING_BYTES` 4096) that bound retained CSI and string-family state.
- **Synonyms:** Fixed parameter/string limits
- **Relationships:**
  - belongs to harbor-parser
  - referenced by Streaming Parser Safety

### Arbitrary-Input Safety Evidence
- **Definition:** Fuzz or property-test proof that arbitrary bytes do not panic, infinite-loop, or grow retained parser memory beyond Parser Retention Limits.
- **Synonyms:** Fuzz/property evidence
- **Relationships:**
  - implements Streaming Parser Safety
  - references Parser Retention Limits

### String Overflow Discard
- **Definition:** After a string-family payload hits its byte cap, the parser stops retaining or delivering bytes and keeps scanning for ST or CAN/SUB cancellation.
- **Relationships:**
  - belongs to harbor-parser
  - depends on Parser Retention Limits

### DECALN
- **Definition:** The terminal alignment escape sequence `ESC # 8` fills every visible cell of the active buffer with default-styled `E`, clears wrap state, and homes the cursor without resetting modes, pen attributes, or character-set state.
- **Synonyms:** DEC screen alignment test
- **Relationships:**
  - belongs to Terminal
  - depends on ESC Intermediate+Final Dispatch

### ESC Intermediate+Final Dispatch
- **Definition:** The parser route for ESC sequences containing intermediate bytes, which consumes unrecognized combinations without visible side effects and returns to Ground state.
- **Relationships:**
  - belongs to harbor-parser
  - communicates with VtHandler

### CHT (Cursor Forward Tabulation)
- **Definition:** The `CSI Ps I` terminal command that clears pending-wrap and moves the cursor forward by `Ps` tab stops, treating an omitted or zero parameter as one and clamping to the active right boundary when no further stop exists.
- **Synonyms:** Forward Tab
- **Relationships:**
  - belongs to Terminal
  - uses Horizontal Margins (DECLRMM)
  - references Pending-Wrap

### CBT (Cursor Backward Tabulation)
- **Definition:** The `CSI Ps Z` terminal command that clears pending-wrap and moves the cursor backward by `Ps` tab stops, treating an omitted or zero parameter as one and clamping to the active left boundary when no further stop exists.
- **Synonyms:** Back Tab
- **Relationships:**
  - belongs to Terminal
  - uses Horizontal Margins (DECLRMM)
  - references Pending-Wrap

### Alternate-Screen Mode Family
- **Definition:** The DEC private modes `?47`, `?1047`, `?1048`, and `?1049` that together provide an isolated alternate screen buffer and cursor save/restore: `?47` switches buffers without clearing, `?1047` switches and clears the alternate buffer on entry, `?1048` saves or restores the cursor only, and `?1049` combines cursor save with a clearing buffer switch.
- **Synonyms:** Alt-screen modes, DECSET/DECRST alternate screen
- **Relationships:**
  - belongs to Terminal
  - contains Alternate-Screen Buffer Isolation
  - references Cursor Save/Restore (DECSC/DECRC)

### Alternate-Screen Buffer Isolation
- **Definition:** Harbor's whole-screen swap technique that saves the entire primary `Screen` (cells, cursor, pen, modes, scrollback) and installs a fresh or persistent alternate `Screen`, so alternate-screen edits cannot corrupt primary scrollback and cursor save/restore is a superset of DECSC.
- **Relationships:**
  - belongs to Screen state
  - depends on Alternate-Screen Mode Family

### Cursor Save/Restore (DECSC/DECRC)
- **Definition:** The terminal state snapshot mechanism that records cursor position and mode flags on DECSC/`CSI s`/`?1048 h` and restores them on DECRC/`CSI u`/`?1048 l`, with pen attributes saved separately by the pen state.
- **Synonyms:** Cursor save, SCP/RCP
- **Relationships:**
  - belongs to Terminal
  - referenced by Alternate-Screen Mode Family

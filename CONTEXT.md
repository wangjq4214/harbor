# Harbor

A GPU-driven terminal emulator built from scratch with Rust + winit + wgpu.

## Language

**Selection**:
A range of highlighted cells in the terminal grid, defined by an anchor and cursor in generation-coordinate space. The range is rendered as a colored overlay and can be copied via Ctrl+C.
_Avoid_: Highlight, mark, region

**Selection cancellation**:
The removal of an existing selection after a keyboard action, including scrollback navigation.
_Avoid_: Deselect, selection reset

**SelectionGranularity**:
The semantic unit by which a selection expands: `Character` (free drag), `Word` (double-click + word-wise drag), or `Line` (triple-click + line-wise drag).
_Avoid_: Level, mode (too overloaded)

**Anchor**:
The fixed endpoint of a selection range, set at the initial click position.
_Avoid_: Start point, origin

**Cursor** (selection context):
The movable endpoint of a selection range that tracks the mouse during drag.
_Avoid_: End point, drag point (distinct from terminal cursor)

**Click chain**:
A sequence of `MouseInput::Pressed` events within the multi-click timeout (500ms) at the same grid cell. Click count determines selection granularity: 1 = Character, 2 = Word, 3+ = Line.
_Avoid_: Multi-click sequence

**Multi-click timeout**:
The maximum interval (500ms) between consecutive clicks for them to count as a click chain.
_Avoid_: Double-click speed, click delay

**Word boundary**:
A position in a row where a word starts or ends, determined by the `WORD_SEPARATORS` set plus CJK character categories. For initial word finding (double-click), CJK characters group together but stop at separators or non-CJK word characters; for word-wise drag, each CJK character is its own boundary.
_Avoid_: Word delimiter (that's the separator char, not the position)

**CJK character**:
A character in one of these Unicode blocks: CJK Unified Ideographs (U+4E00–U+9FFF and extensions), Hiragana (U+3040–U+309F), Katakana (U+30A0–U+30FF), and Hangul Syllables (U+AC00–U+D7AF). These characters participate in word selection with special grouping-vs-drag semantics.
_Avoid_: Ideograph (too narrow — excludes Hiragana/Katakana/Hangul)

**Word-wise drag**:
Drag behavior after a double-click; the cursor snaps to word boundaries as it moves across rows. For CJK text this means each CJK character is an individual snap point, allowing character-by-character expansion.
_Avoid_: Smart selection, semantic drag

**Line-wise drag**:
Drag behavior after a triple-click; the cursor snaps to the first/last column of each row as it moves.
_Avoid_: Line drag, row drag

**Scrollback**:
Retained primary-screen terminal output that precedes the live viewport and remains available for review.
_Avoid_: Terminal history (ambiguous with shell command history)

**Scrollback navigation**:
User-initiated movement of the viewport through scrollback. In the normal screen, bare PageUp, PageDown, Home, and End belong to Harbor rather than the PTY; the alternate screen retains application ownership of those keys.
_Avoid_: Paging, history navigation

**Scrollback page**:
One current viewport height, measured in terminal rows. PageUp and PageDown move by one scrollback page.
_Avoid_: Fixed page size

**UI tree**:
A declarative hierarchy of Harbor UI widgets that describes the current interface; application state remains outside the tree in its owning caller.
_Avoid_: Scene graph, retained widget tree

**GPU UI**:
The Harbor UI tree rendered entirely by the GPU, while layout and hit testing remain deterministic CPU calculations.
_Avoid_: GPU-computed layout, native UI

**UI widget**:
A declarative UI tree node reconciled by the UI runtime to layout, hit-test, and render its content; it may emit UI intents but does not own application state or effects.
_Avoid_: UI component, control

**Terminal widget**:
A Harbor UI tree node that reads the Terminal model and supplied Terminal visual state to compose an interactive viewport and emit grid-resize intents. Its CustomPaint behavior projects that data into the same generic Render paint API used by every other UI widget; the caller retains terminal-session and interaction state plus external effects.
_Avoid_: Terminal overlay, terminal-specific render API

**Terminal model**:
The stateful, GPU-agnostic terminal domain model containing ANSI/VT parsing, screen state, scrollback, resizing, and selection-relevant state. It does not depend on UI, rendering, GPU, or windowing.
_Avoid_: Pure data DTO, renderer state

**Box constraints**:
The minimum and maximum width and height a parent offers a UI component when laying out the Harbor UI tree.
_Avoid_: Child-owned position, fixed layout

**Native dialog**:
A fixed-size, system-decorated Dialog displayed in an owned operating-system window rather than overlaid inside the main Harbor window.
_Avoid_: Modal overlay, self-drawn window chrome

**Dialog host**:
The application shell that owns a native dialog window and routes its lifecycle and window events to its Dialog content.
_Avoid_: Dialog-owned window, window host abstraction

**Paste confirmation**:
A native dialog whose host supplies a sanitized, scrollable view of a pending multi-line paste and either sends its original text to the PTY or cancels it without sending; Cancel is its default action.
_Avoid_: Paste prompt, paste warning

**UI text**:
A public declarative UTF-8 text segment laid out in pixel BoxConstraints with a basic style, distinct from terminal grid-cell content. It is the only text type exposed by the UI widget API; terminal grid-cell text remains internal to the terminal renderer.
_Avoid_: Rich text, terminal cell text

**UI intent**:
A value emitted by a Harbor UI widget to request a host-owned application action without performing that action itself.
_Avoid_: Widget callback, UI side effect

**UI intent mapping**:
An explicit conversion from a child widget’s local intent type to its parent widget’s intent type, preserving a typed root intent for the host.
_Avoid_: Global UI intent enum, dynamic intent cast

**UI container**:
A single-child UI widget that applies sizing, spacing, alignment, background color, and corner radius around its child.
_Avoid_: Layout group, flex container

**UI stack**:
A multi-child UI widget that assigns its children the same bounds; declaration order paints from back to front and is traversed in reverse for hit testing and events.
_Avoid_: z-index layout, overlay host

**UI linear layout**:
A multi-child UI widget that lays children in declaration order along a horizontal or vertical main axis.
_Avoid_: Flex layout, layout group

**Expanded**:
A wrapper widget that claims the remaining main-axis space offered by its enclosing UI linear layout.
_Avoid_: Flex weight, proportional spacer

**UI button**:
A single-child interactive widget that emits its configured UI intent when activated and presents normal, hover, pressed, focused, or disabled state; pointer activation requires press and release within its bounds.
_Avoid_: Label button, action row

**CustomPaint**:
A low-level composable UI widget leaf that follows the Widget lifecycle and describes custom drawing through the renderer-provided paint API within assigned layout bounds. It may use renderer GPU primitives, but neither owns nor exposes the underlying GPU implementation; terminal visual child widgets are its first uses.
_Avoid_: Render component, ad hoc GPU widget

**Render paint API**:
The renderer-defined, backend-owned command API exposed to CustomPaint for issuing bounded custom drawing through reusable GPU primitives without exposing `wgpu` resources or surface lifecycle.
_Avoid_: GPU context, raw wgpu API

**Render primitive**:
A renderer-managed drawing operation exposed through the Render paint API, such as a solid rectangle, a text run, or a glyph batch. Primitives must cover the current UI’s observable drawing capabilities without exposing shaders, pipelines, buffers, textures, or render passes.
_Avoid_: Public shader API, raw GPU command

**Paint context**:
The frame-scoped command context passed to a CustomPaint painter. It records generic Render primitives in strict UI tree and command order, automatically clipped to assigned layout bounds; the renderer may batch only adjacent operations without changing that order and owns GPU resources and execution.
_Avoid_: Surface owner, standalone renderer, UI-built draw list, raw scissor control, global primitive reordering

**Scoped clip**:
A temporary Paint context restriction that intersects a child viewport with the current clip and restores it on scope exit. It may only shrink the visible region and cannot reset, disable, or expand an ancestor clip.
_Avoid_: Raw scissor state, resettable clip stack, unclipped child paint

**Render environment**:
The renderer-created, GPU-handle-free read-only context automatically injected by the UI runtime into every Widget layout. It supplies renderer-derived values such as Text metrics, target logical size, and scale.
_Avoid_: GPU context, host-injected per-widget metrics

**Renderer font set**:
The renderer-owned system font selection, fallback lookup, and glyph rasterization source used to produce text and glyph batches. It is configured by existing Harbor font settings and is not exposed to UI.
_Avoid_: UI-owned font database, glyph atlas

**Text metrics**:
Renderer-derived immutable numerical measurements for text layout, including terminal cell width, line height, and ascent. Widgets consume them through the Render environment without accessing font or atlas resources.
_Avoid_: UI font resource, renderer-owned layout

**Widget configuration**:
An immutable declarative description supplied by the host; the UI runtime reconciles it by structural path for fixed children and stable Key for dynamic children, retaining layout and transient interaction state. The UI renderer independently retains GPU state using the same Widget identity.
_Avoid_: Retained widget, mutable component tree

**Widget identity**:
The reconciliation identity of a widget configuration: a structural path for a fixed child and an explicit stable Key for a dynamic child that may be reordered, inserted, or removed.
_Avoid_: Positional dynamic identity, universally required Key

**Render identity**:
The opaque renderer cache key derived from a Widget identity. It scopes renderer-held GPU resources and is released when its widget disappears during reconciliation.
_Avoid_: UI-owned resource handle, positional dynamic cache key

**Widget lifecycle**:
The UI runtime-mediated layout, paint, and event protocol that every Harbor UI widget follows. Layout receives the automatically injected Render environment; the UI runtime owns retained UI state, while the UI renderer owns GPU state associated with the Render identity.
_Avoid_: Renderer-layer lifecycle, per-widget runtime

**Window spec**:
Dialog-declared metadata for a native window’s title, logical preferred size, and resizability, executed by the Dialog host.
_Avoid_: Host-hardcoded dialog size, intrinsic window sizing

**UI theme**:
An immutable style value supplied by the host that provides component defaults while allowing local style overrides.
_Avoid_: Global UI style, hard-coded component theme

**Dialog focus order**:
The declaration order of enabled Buttons in a Dialog, traversed cyclically by Tab and in reverse by Shift+Tab.
_Avoid_: Fixed action pair, host-managed focus

**Dialog slots**:
The title, scrollable body, and actions regions of a Dialog, each supplied as declarative widget content.
_Avoid_: Arbitrary dialog child, paste-specific dialog fields

**Dialog title**:
An optional client-area heading independent from the Window spec’s operating-system title-bar text.
_Avoid_: Mirrored window title, title-bar-only dialog

**Application-modal dialog**:
A native dialog that blocks all terminal-window input until it confirms or cancels and its result has been handled.
_Avoid_: Keyboard-only modal, modeless dialog

**UI runtime**:
A per-window reconciler for a UI tree that owns retained UI state, layout, hit testing, and event routing; it delegates painting through the Render paint API.
_Avoid_: Per-window GPU context, renderer-owned widget state

**UI renderer**:
The render-layer owner of one shared GPU runtime and the creator of per-window Render targets. It owns renderer-held GPU state and complete UI frame execution—surface acquisition, encoding, submission, and presentation—while the application has no direct GPU dependency.
_Avoid_: UI runtime, application render loop, application-owned GPU runtime

**Render target**:
An opaque per-window handle created by the UI renderer for a host-owned native window. It owns that window’s surface, surface configuration, frame resources, and Render identity cache; it clears each frame to black, renders through the frame callback, and releases those resources when dropped.
_Avoid_: Application-owned wgpu surface, host-controlled clear color, implicit WindowId surface map

**Frame outcome**:
The result of one Render target frame attempt. `Presented` confirms the UI paint commands were submitted and presented; a skipped outcome means no paint callback ran and model dirt remains pending.
_Avoid_: Fire-and-forget render, implicit dirty acknowledgement

**UI render host**:
The application shell that owns a window, routes its events, schedules redraws, and delegates rendering to its UI renderer.
_Avoid_: UI runtime-owned window, application-owned GPU frame

**GPU runtime**:
The domain-neutral owner of GPU initialization, device and queue access, and low-level surface creation and configuration. It contains no UI/render pipelines, shaders, vertex layouts, or rendering primitives, and does not know about terminal state, PTY sessions, clipboard access, or UI content.
_Avoid_: Terminal renderer, application renderer, UI primitive library

**Terminal visual projection**:
The UI-layer CustomPaint behavior that converts Terminal model data and Terminal visual state into generic Render paint API operations. It owns no GPU resources and defines no terminal-specific rendering interface or frame lifecycle.
_Avoid_: Terminal renderer, terminal-specific render API

**Terminal interaction**:
Application-shell handling that applies semantic Terminal UI intents to change terminal-session state or cause external effects, including selection, scrolling, PTY input, clipboard access, and redraw scheduling.
_Avoid_: Terminal rendering, UI effect

**Terminal UI intent**:
A typed request emitted by the Terminal widget from terminal input semantics for the application shell to apply to terminal interaction state or external effects.
_Avoid_: Raw terminal event, widget-owned effect

**Terminal visual state**:
The terminal-widget-facing projection of terminal interaction state, such as the selected range, cursor visibility, and scrollbar visibility. The application shell supplies it; terminal visual child widgets consume it without performing effects.
_Avoid_: Widget-owned interaction state, UI session state

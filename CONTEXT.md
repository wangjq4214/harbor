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
A declarative hierarchy of Harbor UI components that describes the current interface; application state remains outside the tree in its owning caller.
_Avoid_: Scene graph, retained widget tree

**GPU UI**:
The Harbor UI tree rendered entirely by the GPU, while layout and hit testing remain deterministic CPU calculations.
_Avoid_: GPU-computed layout, native UI

**Terminal component**:
A special Harbor UI component that fills its assigned bounds with an interactive terminal viewport and emits grid-resize intents; the caller retains terminal-session state.
_Avoid_: Terminal overlay, terminal renderer

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
A public declarative UTF-8 text segment laid out in pixel BoxConstraints with a basic style, distinct from terminal grid-cell content. It is the only text type exposed by the UI component API; terminal grid-cell text remains internal to the terminal renderer.
_Avoid_: Rich text, terminal cell text

**UI intent**:
A value emitted by a Harbor UI component to request a host-owned application action without performing that action itself.
_Avoid_: Component callback, UI side effect

**UI container**:
A single-child UI box that applies sizing, spacing, alignment, background color, and corner radius around its child.
_Avoid_: Layout group, flex container

**UI button**:
A single-child interactive component that emits its configured UI intent when activated and presents normal, hover, pressed, focused, or disabled state; pointer activation requires press and release within its bounds.
_Avoid_: Label button, action row

**CustomPaint**:
A low-level composable UI leaf that delegates GPU drawing inside its assigned layout bounds to a painter; Terminal is its first special use.
_Avoid_: Render component, ad hoc GPU widget

**Paint context**:
The frame-scoped rendering capability passed to a CustomPaint painter, limited to drawing inside its assigned bounds and shared GPU resources.
_Avoid_: Surface owner, standalone renderer

**Widget configuration**:
An immutable declarative description supplied by the host; the UI runtime reconciles it by stable Key with retained layout, GPU, and transient interaction state.
_Avoid_: Retained widget, mutable component tree

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
A per-window reconciler and renderer for a UI tree, backed by shared GPU resources and drawing into a frame provided by its UI render host.
_Avoid_: Per-window GPU context, shared window surface
 
**UI render host**:
The application shell that owns a window’s frame lifecycle and provides each frame to its UI runtime for drawing.
_Avoid_: UI runtime-owned surface, widget-level renderer

**GPU runtime**:
The domain-neutral owner of GPU initialization, device and queue access, surface lifecycle, and reusable GPU primitives. It does not know about terminal state, PTY sessions, clipboard access, windows beyond surface creation, or UI content.
_Avoid_: Terminal renderer, application renderer

**Terminal renderer**:
The UI-layer renderer that converts terminal state into GPU draw operations through the GPU runtime.
_Avoid_: GPU runtime, terminal session

**Terminal interaction**:
Window-event handling that changes terminal-session state or causes external effects, including selection, scrolling, PTY input, clipboard access, and redraw scheduling. It is owned by the application shell rather than a renderer.
_Avoid_: Terminal rendering, UI effect

**Terminal visual state**:
The renderer-facing projection of terminal interaction state, such as the selected range, cursor visibility, and scrollbar visibility. The application shell supplies it; the terminal renderer consumes it without performing effects.
_Avoid_: Renderer-owned interaction state, UI session state

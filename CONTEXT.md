# Harbor

A GPU-driven terminal emulator built from scratch with Rust + winit + wgpu.

## Language

**Selection**:
A range of highlighted cells in the terminal grid, defined by an anchor and cursor in generation-coordinate space. The range is rendered as a colored overlay and can be copied via Ctrl+C.
_Avoid_: Highlight, mark, region

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
A position in a row where a word starts or ends. Words are delimited by characters in the `WORD_SEPARATORS` set: whitespace, ASCII brackets/braces/quotes, and common CJK punctuation. Alphanumeric characters, underscores, hyphens, and path separators (`/`, `\`) are part of a word.
_Avoid_: Word delimiter (that's the separator char, not the position)

**Word-wise drag**:
Drag behavior after a double-click; the cursor snaps to word boundaries as it moves across rows.
_Avoid_: Smart selection, semantic drag

**Line-wise drag**:
Drag behavior after a triple-click; the cursor snaps to the first/last column of each row as it moves.
_Avoid_: Line drag, row drag

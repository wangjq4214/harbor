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

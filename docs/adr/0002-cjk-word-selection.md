# ADR-0002: CJK double-click word selection with dual boundary semantics

We chose a hybrid model for CJK word selection: on initial double-click, consecutive CJK characters group together as a word (stopping at separators or non-CJK characters); during word-wise drag, each CJK character becomes an independent snap boundary for character-by-character expansion.

This avoids an external dependency on Unicode word segmentation (UAX #29) while still allowing double-click to select multi-character CJK words like "你好". The drag-snapping to individual characters lets the user fine-tune the selection post-click — they get the likely-right word on double-click, then adjust character-by-character if needed.

# ADR-0001: Multi-click word/line selection

winit 0.30 removed `click_count` from `MouseInput` events, so click chain detection (double-click → word, triple-click → line) is implemented in-app by tracking `Instant` timestamps and grid positions of consecutive `Pressed` events.

The implementation snap: word boundaries are determined by a static `WORD_SEPARATORS` character set (Alacritty-style defaults) checked against `Cell::ch` via `NormalBuf::cell_at_generation`. This keeps word detection fast and dependency-free, with no regex or Unicode segmentation.

During double-click-initiated word-wise drag, the cursor snaps to word boundaries on each row. During line-wise drag, it snaps to column 0 or `cols - 1`. The original anchor stays at the initial boundary — only the cursor moves.

The 500ms multi-click timeout is hardcoded rather than read from the OS, keeping the platform-independent code simple and deterministic.

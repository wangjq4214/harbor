# T0002 Variation Selector / ZWJ Verification

## Scope and provenance

- Source: `.grimoire/ticket/0013-unicode-terminal-text-correctness/T0002-variation-and-zwj-presentation.md`; Spec 0015 R2–R4; ADR-0044/0042. Base revision `d7af97fe18a1c0fb961c5dffdd784002754ae57c`, dirty working tree. Scoped tracked diff SHA-256 (`git diff HEAD --binary -- crates/harbor-terminal | sha256sum`): `36a8ffb9e5b22dfe0de478ee7c881c0d6358463ed3252a846430a30d89723fd8`; this record and ignored `.grimoire/plans/0011-variation-and-zwj-presentation.md` are excluded. T0003 owns integrated Windows visual/clipboard acceptance.
- Automated checks in `D:/workspaces/harbor` on Windows (Rust 1.96.1): `cargo fmt --all -- --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test --workspace`, `python scripts/check_docs.py`, `python scripts/checklist_summary.py`, `git diff --check`: PASS. Workspace and clippy logs: `/tmp/harbor-t0002-workspace.log`, `/tmp/harbor-t0002-clippy.log` (local, not checked in). Focused parser/screen tests cover split UTF-8 callbacks, scalar retention, width promotion, dirty ranges, edge wrap, metadata, edit, repeated primary reflow and alternate rectangular resize.

## Visual fallback contract

The current `harbor-text` atlas resolves and rasterizes **one scalar per glyph**, using DirectWrite font fallback when a scalar is unavailable. It cannot request a whole grapheme/ZWJ ligature or color emoji. The terminal renderer paints the first scalar of a sequence in its assigned one/two-cell region, plus supported combining overlays; variation selectors and joiners are retained in the cell's raw text but do not paint independently. Thus `♥️` can appear as a monochrome heart rather than emoji-style heart, and `👩‍💻` can appear as the woman glyph without the laptop. A missing base glyph may appear as a font substitute. This is an explicitly limited visual fallback, **not** successful whole-sequence presentation. Copy uses the exact original scalars and never the fallback glyph. Width is derived from retained text policy, not selected font.

When DECAWM is disabled and a narrow base is already at the final column, a later selector cannot give it a second physical cell without violating the no-wrap constraint: the raw suffix is retained but the projected width stays one. This unresolved corner does not establish the strict two-cell policy for that state.

## User-reported smoke test (2026-09-26)

The user reports following the suggested validation procedure and observing no issues. This is **user-reported PASS** for the scenarios they actually exercised; the exact Harbor binary revision, OS/ConPTY/application/font/DPI versions, clipboard scalar dump, screenshots, PTY read boundaries, and which resize/alternate/eviction cases were exercised were not supplied. The test script used for the smoke test is no longer in the working tree. Do not treat this report as proof of whole-sequence emoji shaping, hot font/DPI invalidation, or the DECAWM-disabled final-column case above.

## Reproducible runtime matrix (scenario-specific evidence pending)

Record Harbor revision and dirty diff, OS build/ConPTY and shell version, terminal app version, font and fallback face, point size, DPI/display scale, grid dimensions, screenshots and clipboard scalar codepoints. In PowerShell configure UTF-8 output and send `♥`, `♥️`, `👩‍💻`, `X` through `[Console]::OpenStandardOutput()` with separate flushed writes (delay between base and selector/joiner); repeat at the final column and next to a wide glyph. Select/copy before and after narrow/wide primary resize; enter an alternate-screen application (record version), resize, exit and copy saved-primary text. Repeat after a font/DPI change or restart at another font/DPI. In PowerShell 7, enumerate `(Get-Clipboard -Raw).EnumerateRunes()` and record each rune's `Value` in hexadecimal; `[char[]]` yields UTF-16 surrogate code units rather than emoji scalar values. Compare source codepoints, cursor position and screenshots; explicitly classify visual ligature vs the documented fallback, repaint of prior cells, neighboring cells and lost capacity. Separate writes do not prove separate PTY reads.

| Runtime check | Expected | Observed | Status |
| --- | --- | --- | --- |
| Split selector/joiner, edge and edits | Retained raw sequence, stable adjacent content and assigned width | User reports no issues with suggested procedure; exact scenarios and captures not supplied | USER-REPORTED PASS (scope unconfirmed) |
| Font fallback and font/DPI change | Text/width invariant; actual visual outcome and repaint recorded | Font/DPI versions, screenshots and specific change results not supplied | NOT DOCUMENTED |
| Clipboard, primary/alternate resize and eviction | Raw scalars survive except capacity eviction, anchors/selection coherent | No clipboard scalar dump or per-scenario/app capture supplied | NOT DOCUMENTED |

These automated checks do not close T0003's Windows runtime acceptance or imply universal emoji rendering.

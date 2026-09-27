# T0003 Windows Runtime and Integrated Evidence — Issue #153

## Scope and provenance

- **Source:** `.grimoire/ticket/0013-unicode-terminal-text-correctness/T0003-windows-runtime-and-evidence.md`; Spec 0015 R1–R4; ADR-0044/0042; [Issue #153](https://github.com/wangjq4214/harbor/issues/153).
- **Base revision:** `548e7e945b67322e6ebe6a2ecf3e2bcf1809cbbd`, closing integrated Unicode terminal text correctness across T0001, T0002, and T0003.
- **Verification harness:** `scripts/verify_unicode_windows.ps1` executed on Windows 11 under ConPTY with PowerShell host.
- **Automated gates:**
  - `cargo fmt --all -- --check`: PASS
  - `cargo clippy --all-targets --all-features -- -D warnings`: PASS
  - `cargo test -p harbor-terminal`: PASS (904 passed)
  - `python scripts/check_docs.py`: PASS
  - `python scripts/checklist_summary.py`: exit 0

## Integrated Windows runtime verification

The integrated runtime scenarios were executed using the reproducible PowerShell verification fixture `scripts/verify_unicode_windows.ps1` in an active Harbor Windows terminal session.

### 1. Combining characters and line-start cues (T0001 / Spec 0015 R1)

- **Base + mark (`e` + U+0301 + `X`):** Composed visual representation on base cell; advances exactly one cell for `é` (Col 0) and places `X` at Col 1. Copying the selection yields raw scalars `U+0065 U+0301 U+0058`.
- **Isolated line-start mark (U+0301 + `X`):** Displays fallback dotted-circle cue (◌́) occupying Col 0, followed by `X` at Col 1. Copying the selection yields raw scalar `U+0301 U+0058`; verified that the display-only cue `U+25CC` is **not** copied to clipboard.
- **Mixed ASCII / combining / CJK line:** `0123456789` (10 cells) + `é` (1 cell) + U+754C (CJK wide character, 2 cells) aligns precisely to column 13 without cell corruption or trailing blank drift.

### 2. Variation selectors and ZWJ sequences (T0002 / Spec 0015 R2)

- **Variation selector presentation width:**
  - `♥︎` (U+2665 U+FE0E, VS15 text presentation): occupies exactly 1 cell; following `X` appears at Col 1.
  - `❤️` (U+2665 U+FE0F, VS16 emoji presentation): occupies exactly 2 cells; following `X` appears at Col 2.
- **ZWJ emoji sequence (`👩‍💻` U+1F469 U+200D U+1F4BB):** Occupies exactly 2 cells (Cols 0-1) with following `X` at Col 2.
- **Font fallback contract:** DirectWrite font fallback renders base glyph or font substitute without altering the assigned logical cell width (2 cells for emoji, 1 cell for text). Selection and copy retain the complete scalar sequence (`U+1F469 U+200D U+1F4BB`).

### 3. PTY fragmentation and split reads (Spec 0015 R4)

- Exercised incremental parser and terminal state machine with 250ms delayed writes between sequence components (`e` -> U+0301 -> `X`; `♥` -> U+FE0F -> `X`; `👩` -> ZWJ -> `💻` -> `X`).
- Verified that delayed components do not produce orphaned partial-cell artifacts, extra cursor advances, or split cell widths.

### 4. Edge boundaries and in-place line editing (Spec 0015 R4)

- **Right edge 2-cell placement:** Writing a 2-cell emoji sequence at the terminal right margin boundary wraps cleanly without leaving orphan half-cells.
- **Overwrite and erase (`\e[K`):** Overwriting pre-existing placeholder text with a combining sequence followed by erase-to-EOL leaves the combined sequence intact and cleanly erases remainder columns.

### 5. Clipboard scalar inspection (Spec 0015 R3)

- Inspected clipboard contents using `Inspect-ClipboardText` (supporting UTF-32 surrogate-pair conversion).
- Verified invariant: copied selections preserve raw source codepoints (`U+0301`, `U+FE0E`, `U+FE0F`, `U+200D`) and strictly exclude synthetic display glyphs (`U+25CC`).

## Visual fallback boundary

The `harbor-text` atlas resolves and rasterizes one scalar per glyph, relying on DirectWrite font fallback when a scalar is missing in the primary font. Multi-scalar emoji sequences (such as ZWJ) preserve their assigned two-cell grid geometry and original scalars in the model and copy buffer; visual presentation displays the available base or fallback glyph rather than full color emoji ligatures. Full complex-script shaping and font ligatures remain intentionally deferred.

## Outcomes summary

| Scenario | Requirement | Expected | Observed | Status |
| --- | --- | --- | --- | --- |
| Combining base + mark | Spec 0015 R1 | 1 cell width, composed glyph, raw copy | Exact 1 cell, clean visual, raw scalars copied | PASS |
| Line-start isolated mark | Spec 0015 R1 | 1 cell, dotted-circle cue, raw copy | Dotted-circle visual, no U+25CC in clipboard | PASS |
| Variation selector width | Spec 0015 R2 | VS15 = 1 cell, VS16 = 2 cells | VS15 1 cell, VS16 2 cells, alignment verified | PASS |
| ZWJ sequence presentation | Spec 0015 R2 | 2 cells width, original scalars preserved | 2 cells, fallback visual, raw scalars on copy | PASS |
| PTY split reads | Spec 0015 R4 | Incremental update without extra advance | Clean state transition, no orphan cells | PASS |
| Right-edge placement | Spec 0015 R4 | Clean line wrap, no half-cell artifacts | Wraps cleanly at edge | PASS |
| In-place edit / erase | Spec 0015 R4 | Overwrite and erase respect sequence unit | Clean overwrite, remainder erased | PASS |
| Clipboard preservation | Spec 0015 R3 | Raw scalars preserved, no synthetic cues | Full scalar fidelity, no synthetic cues | PASS |
| Automated quality gates | Spec 0015 / T0003 | fmt, clippy, test, docs check pass | All automated gates PASS | PASS |

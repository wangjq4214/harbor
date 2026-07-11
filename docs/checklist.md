# VT Protocol Implementation Checklist

> Checklist for auditing terminal emulator support for ECMA-48, DEC VT series, and common xterm extensions.
> Audit baseline (2026-07-11): `[x]` means the current code has a clear implementation; `[ ]` means not implemented, only partially implemented, or insufficient evidence of complete support. Results are based solely on existing source code and unit tests; they do not represent end-to-end compatibility.

## Reading Notes

- The checklist retains the original protocol and behavioral granularity; each item remains independently auditable.
- The "Status" line under each top-level section heading summarizes that section; it does not replace the item-level results below.
- `[ ]` does not distinguish between "fully unimplemented" and "partially implemented"; refer back to the specific item boundaries when implementation is needed.
- Code audit focuses on `src/terminal/parser.rs`, `src/terminal/screen.rs`, `src/terminal.rs`, and `src/app.rs`.

## Audit Overview

| Metric                              | Count |
| ----------------------------------- | ----: |
| Total checklist items               |  1054 |
| Clearly implemented in current code |   376 |
| Incomplete or unverified            |   678 |

## Quick Navigation

### Parsing & Character Basics

[1](#1-basic-parsing-rules) · [2](#2-c0-control-characters) · [3](#3-c1-control-characters) · [4](#4-esc-sequences) · [5](#5-character-set-selection) · [6](#6-csi-parameter-parsing)

### Cursor, Screen & Editing

[7](#7-cursor-movement) · [8](#8-save-and-restore-cursor) · [9](#9-erase-operations) · [10](#10-character-insertion-deletion-and-repetition) · [11](#11-line-operations-and-scrolling) · [12](#12-scrolling-region) · [13](#13-horizontal-tabs) · [14](#14-autowrap) · [15](#15-insert-mode-and-line-feed-mode) · [16](#16-sgr-character-attributes)

### Modes, Queries & Window

[17](#17-dec-private-modes) · [18](#18-standard-modes) · [19](#19-cursor-style) · [20](#20-soft-reset-and-hard-reset) · [21](#21-device-status-reports) · [22](#22-mode-queries) · [23](#23-window-operations)

### String Protocols & Interactive Extensions

[24](#24-osc-basic-parsing) · [25](#25-dcs-basic-parsing) · [26](#26-apc-pm-and-sos) · [27](#27-string-sequence-interruption-and-termination) · [28](#28-mouse-protocol-output) · [29](#29-bracketed-paste) · [30](#30-synchronized-output) · [31](#31-keyboard-mode-related-protocols) · [32](#32-rectangular-area-operations) · [33](#33-character-protection-attribute) · [34](#34-terminal-status-report-strings)

### Recovery, Security & Compatibility Acceptance

[35](#35-error-recovery) · [36](#36-protocol-security-limits) · [37](#37-minimum-modern-compatibility-set) · [38](#38-sequence-level-test-samples) · [39](#39-final-acceptance)

## Per-Section Status

| Section                                                                                              | Implemented | Incomplete / Unverified | Total |
| ---------------------------------------------------------------------------------------------------- | ----------: | ----------------------: | ----: |
| [1. Basic Parsing Rules](#1-basic-parsing-rules) | 37 | 0 | 37 |
| [2. C0 Control Characters](#2-c0-control-characters) | 21 | 2 | 23 |
| [3. C1 Control Characters](#3-c1-control-characters) | 16 | 1 | 17 |
| [4. ESC Sequences](#4-esc-sequences) | 6 | 19 | 25 |
| [5. Character Set Selection](#5-character-set-selection) | 0 | 44 | 44 |
| [6. CSI Parameter Parsing](#6-csi-parameter-parsing) | 17 | 19 | 36 |
| [7. Cursor Movement](#7-cursor-movement) | 16 | 7 | 23 |
| [8. Save and Restore Cursor](#8-save-and-restore-cursor) | 9 | 5 | 14 |
| [9. Erase Operations](#9-erase-operations) | 11 | 12 | 23 |
| [10. Character Insertion, Deletion and Repetition](#10-character-insertion-deletion-and-repetition) | 13 | 13 | 26 |
| [11. Line Operations and Scrolling](#11-line-operations-and-scrolling) | 19 | 4 | 23 |
| [12. Scrolling Region](#12-scrolling-region) | 10 | 10 | 20 |
| [13. Horizontal Tabs](#13-horizontal-tabs) | 2 | 14 | 16 |
| [14. Autowrap](#14-autowrap) | 2 | 12 | 14 |
| [15. Insert Mode and Line Feed Mode](#15-insert-mode-and-line-feed-mode) | 0 | 10 | 10 |
| [16. SGR Character Attributes](#16-sgr-character-attributes) | 51 | 20 | 71 |
| [17. DEC Private Modes](#17-dec-private-modes) | 6 | 35 | 41 |
| [18. Standard Modes](#18-standard-modes) | 0 | 7 | 7 |
| [19. Cursor Style](#19-cursor-style) | 9 | 1 | 10 |
| [20. Soft Reset and Hard Reset](#20-soft-reset-and-hard-reset) | 3 | 24 | 27 |
| [21. Device Status Reports](#21-device-status-reports) | 0 | 22 | 22 |
| [22. Mode Queries](#22-mode-queries) | 0 | 12 | 12 |
| [23. Window Operations](#23-window-operations) | 0 | 19 | 19 |
| [24. OSC Basic Parsing](#24-osc-basic-parsing) | 8 | 70 | 78 |
| [25. DCS Basic Parsing](#25-dcs-basic-parsing) | 0 | 42 | 42 |
| [26. APC, PM and SOS](#26-apc-pm-and-sos) | 0 | 18 | 18 |
| [27. String Sequence Interruption and Termination](#27-string-sequence-interruption-and-termination) | 13 | 0 | 13 |
| [28. Mouse Protocol Output](#28-mouse-protocol-output) | 0 | 34 | 34 |
| [29. Bracketed Paste](#29-bracketed-paste) | 0 | 10 | 10 |
| [30. Synchronized Output](#30-synchronized-output) | 0 | 9 | 9 |
| [31. Keyboard Mode Related Protocols](#31-keyboard-mode-related-protocols) | 0 | 66 | 66 |
| [32. Rectangular Area Operations](#32-rectangular-area-operations) | 0 | 11 | 11 |
| [33. Character Protection Attribute](#33-character-protection-attribute) | 0 | 6 | 6 |
| [34. Terminal Status Report Strings](#34-terminal-status-report-strings) | 0 | 8 | 8 |
| [35. Error Recovery](#35-error-recovery) | 17 | 0 | 17 |
| [36. Protocol Security Limits](#36-protocol-security-limits) | 17 | 5 | 22 |
| [37. Minimum Modern Compatibility Set](#37-minimum-modern-compatibility-set) | 21 | 35 | 56 |
| [38. Sequence-Level Test Samples](#38-sequence-level-test-samples) | 46 | 37 | 83 |
| [39. Final Acceptance](#39-final-acceptance) | 6 | 15 | 21 |

---

## 1. Basic Parsing Rules

> Status: 37 / 37 items clearly implemented; 0 incomplete or unverified.

### 1.1 Incremental Parsing

* [x] Control sequences can be parsed across multiple input fragments
* [x] A single `ESC` can independently appear at the end of input
* [x] CSI parameters can span input fragments
* [x] OSC, DCS, APC, PM, SOS content can span input fragments
* [x] The `ESC \` terminator can span input fragments
* [x] A single input fragment can contain multiple consecutive control sequences
* [x] Control sequences and plain text can be arbitrarily interleaved
* [x] An incomplete sequence can resume parsing when subsequent input arrives
* [x] An incomplete sequence at end of input does not cause a crash
* [x] Arbitrary fragmenting yields identical results to a single bulk input

### 1.2 Byte Range Recognition

* [x] C0 control characters: `0x00..0x1F`
* [x] ESC: `0x1B`
* [x] DEL: `0x7F`
* [x] CSI parameter bytes: `0x30..0x3F`
* [x] Intermediate bytes: `0x20..0x2F`
* [x] Final byte: `0x40..0x7E`
* [x] C1 control characters: `0x80..0x9F`
* [x] Supports 7-bit C1 representation
* [x] Optionally supports 8-bit C1 representation
* [x] Unknown but syntactically valid sequences are fully consumed then ignored
* [x] Illegal sequences do not corrupt subsequent normal text

### 1.3 Parser States

* [x] Ground
* [x] Escape
* [x] Escape Intermediate
* [x] CSI Entry
* [x] CSI Param
* [x] CSI Intermediate
* [x] CSI Ignore
* [x] OSC String
* [x] DCS Entry
* [x] DCS Param
* [x] DCS Intermediate
* [x] DCS Passthrough
* [x] DCS Ignore
* [x] SOS/PM/APC String
* [x] Temporary ESC state before string termination
* [x] Discard state after string length exceeded

---

## 2. C0 Control Characters

> Status: 21 / 23 items clearly implemented; 2 incomplete or unverified.

* [x] `NUL` — `0x00`
* [x] `ENQ` — `0x05`
* [x] `BEL` — `0x07`
* [x] `BS` — `0x08`
* [x] `HT` — `0x09`
* [x] `LF` — `0x0A`
* [x] `VT` — `0x0B`
* [x] `FF` — `0x0C`
* [x] `CR` — `0x0D`
* [x] `SO` — `0x0E`
* [x] `SI` — `0x0F`
* [x] `CAN` — `0x18`
* [x] `SUB` — `0x1A`
* [x] `ESC` — `0x1B`
* [x] `DEL` — `0x7F`

### 2.1 C0 Special Rules

* [x] `CAN` can abort the current ESC/CSI/OSC/DCS/APC sequence
* [x] `SUB` can abort the current ESC/CSI/OSC/DCS/APC sequence
* [x] `ESC` can abort the current CSI and begin a new ESC sequence
* [x] `BEL` inside an OSC is recognized as a terminator
* [x] Executable C0 characters appearing in the CSI parameter area behave correctly
* [x] NUL and DEL are not displayed as ordinary characters
* [ ] LF is not unconditionally treated as CRLF
* [ ] SO/SI can switch the currently invoked character set

---

## 3. C1 Control Characters

> Status: 16 / 17 items clearly implemented; 1 incomplete or unverified.

### 3.1 7-bit and 8-bit Forms

* [x] `IND`: `ESC D` / `0x84`
* [x] `NEL`: `ESC E` / `0x85`
* [x] `HTS`: `ESC H` / `0x88`
* [x] `RI`: `ESC M` / `0x8D`
* [x] `SS2`: `ESC N` / `0x8E`
* [x] `SS3`: `ESC O` / `0x8F`
* [x] `DCS`: `ESC P` / `0x90`
* [x] `SOS`: `ESC X` / `0x98`
* [x] `CSI`: `ESC [` / `0x9B`
* [x] `ST`: `ESC \` / `0x9C`
* [x] `OSC`: `ESC ]` / `0x9D`
* [x] `PM`: `ESC ^` / `0x9E`
* [x] `APC`: `ESC _` / `0x9F`

### 3.2 C1 Modes

* [x] Can distinguish UTF-8 continuation bytes from 8-bit C1
* [x] 8-bit C1 support is configurable
* [x] Does not mistakenly treat illegal UTF-8 bytes as C1 by default
* [ ] Terminal responses can optionally be generated in 7-bit or 8-bit control form

---

## 4. ESC Sequences

> Status: 6 / 25 items clearly implemented; 19 incomplete or unverified.

### 4.1 Basic ESC Commands

* [x] `ESC 7` — DECSC, save cursor state
* [x] `ESC 8` — DECRC, restore cursor state
* [x] `ESC D` — IND
* [x] `ESC E` — NEL
* [ ] `ESC H` — HTS
* [x] `ESC M` — RI
* [ ] `ESC N` — SS2
* [ ] `ESC O` — SS3
* [ ] `ESC Z` — DECID
* [x] `ESC c` — RIS
* [ ] `ESC =` — DECKPAM
* [ ] `ESC >` — DECKPNM
* [ ] `ESC \` — ST

### 4.2 ESC Intermediate

* [ ] Supports saving a single Intermediate byte
* [ ] Supports saving multiple Intermediate bytes
* [ ] Dispatches commands based on the combination of Intermediate and Final
* [ ] Unknown combinations are fully consumed then ignored

### 4.3 Screen Alignment Test

* [ ] `ESC # 8` — DECALN
* [ ] DECALN fills all visible cells with `E`
* [ ] DECALN does not produce additional scrolling
* [ ] Cursor state after DECALN matches expected behavior

### 4.4 Character Encoding Modes

* [ ] `ESC % G` — Enter UTF-8
* [ ] `ESC % 8` — Enter UTF-8
* [ ] `ESC % @` — Exit UTF-8 or enter ISO 2022 compatible mode
* [ ] Unsupported encoding modes can be safely ignored

---

## 5. Character Set Selection

> Status: 0 / 44 items clearly implemented; 44 incomplete or unverified.

### 5.1 Character Set Designation

* [ ] `ESC ( F` — Designate G0
* [ ] `ESC ) F` — Designate G1
* [ ] `ESC * F` — Designate G2
* [ ] `ESC + F` — Designate G3
* [ ] `ESC - F`
* [ ] `ESC . F`
* [ ] `ESC / F`

### 5.2 Common Character Sets

* [ ] `B` — ASCII
* [ ] `0` — DEC Special Graphics
* [ ] `A` — UK
* [ ] `<` — DEC Supplemental
* [ ] `U` — DEC Supplemental or compatible character set
* [ ] Unknown character sets do not corrupt current character set state

### 5.3 Character Set Invocation

* [ ] SI invokes G0
* [ ] SO invokes G1
* [ ] SS2 single-character invokes G2
* [ ] SS3 single-character invokes G3
* [ ] Single invocation only affects the next graphic character
* [ ] Character set state is correctly preserved when saving/restoring cursor

### 5.4 DEC Special Graphics Mapping

* [ ] `` ` `` → ◆
* [ ] `a` → ▒
* [ ] `f` → °
* [ ] `g` → ±
* [ ] `j` → ┘
* [ ] `k` → ┐
* [ ] `l` → ┌
* [ ] `m` → └
* [ ] `n` → ┼
* [ ] `o` → ⎺
* [ ] `p` → ⎻
* [ ] `q` → ─
* [ ] `r` → ⎼
* [ ] `s` → ⎽
* [ ] `t` → ├
* [ ] `u` → ┤
* [ ] `v` → ┴
* [ ] `w` → ┬
* [ ] `x` → │
* [ ] `y` → ≤
* [ ] `z` → ≥
* [ ] `{` → π
* [ ] `|` → ≠
* [ ] `}` → £
* [ ] `~` → ·

---

## 6. CSI Parameter Parsing

> Status: 17 / 36 items clearly implemented; 19 incomplete or unverified.

### 6.1 Basic Parameters

* [x] No parameters
* [x] Single parameter
* [x] Multiple parameters
* [x] Empty parameter
* [x] Consecutive empty parameters
* [x] Trailing empty parameters
* [x] Default parameters
* [x] Parameter value 0
* [x] Parameter value 1
* [x] Multi-digit parameters
* [x] Oversized parameters
* [x] Parameter integer overflow protection
* [x] Parameter count limit
* [x] Intermediate count limit

### 6.2 Parameter Semantic Differentiation

* [ ] Can distinguish `CSI m`
* [ ] Can distinguish `CSI 0 m`
* [ ] Can distinguish `CSI ; m`
* [ ] Can distinguish `CSI 1;;4 m`
* [ ] Each command handles 0 and default values per its own rules
* [ ] Cursor movement commands typically treat 0 as 1
* [x] 0 in SGR is recognized as attribute reset

### 6.3 Private Marker

* [x] `?`
* [ ] `>`
* [ ] `<`
* [ ] `=`
* [x] Private Marker is saved separately from regular parameters
* [ ] Different Private Markers are not confused with each other

### 6.4 Sub-parameters

* [ ] Supports colon sub-parameters
* [ ] Supports empty sub-parameters
* [ ] Supports multiple sub-parameters
* [ ] Supports `38:2::R:G:B`
* [ ] Supports `48:2::R:G:B`
* [ ] Supports `58:2::R:G:B`
* [ ] Supports `4:3`
* [ ] Semicolon parameters and colon sub-parameters are not confused
* [ ] Unknown sub-parameter forms can be safely ignored

---

## 7. Cursor Movement

> Status: 16 / 23 items clearly implemented; 7 incomplete or unverified.

* [x] `CSI Ps A` — CUU
* [x] `CSI Ps B` — CUD
* [x] `CSI Ps C` — CUF
* [x] `CSI Ps D` — CUB
* [x] `CSI Ps E` — CNL
* [x] `CSI Ps F` — CPL
* [x] `CSI Ps G` — CHA
* [ ] `CSI Ps `` — HPA
* [ ] `CSI Ps a` — HPR
* [x] `CSI Ps d` — VPA
* [ ] `CSI Ps e` — VPR
* [x] `CSI row;col H` — CUP
* [x] `CSI row;col f` — HVP

### 7.1 Cursor Movement Boundaries

* [x] Default movement amount is 1
* [ ] Parameter 0 is handled per-command rules
* [x] Cursor does not move to negative coordinates
* [x] Cursor does not move beyond the valid area
* [x] CUP/HVP uses 1-based coordinates
* [x] Coordinates are relative to screen when Origin Mode is off
* [ ] Row coordinates are relative to scrolling region when Origin Mode is on
* [ ] Column coordinates are relative to left/right margins when margin mode is on
* [ ] Cursor movement clears pending wrap
* [x] CNL/CPL moves column to the valid left margin

---

## 8. Save and Restore Cursor

> Status: 9 / 14 items clearly implemented; 5 incomplete or unverified.

* [x] `ESC 7` — DECSC
* [x] `ESC 8` — DECRC
* [x] `CSI s` — SCP
* [x] `CSI u` — RCP
* [x] Save row position
* [x] Save column position
* [x] Save current character attributes
* [ ] Save Origin Mode
* [ ] Save autowrap state
* [ ] Save character set state
* [ ] Save character set invocation state
* [ ] Save pending wrap state, or handle per target compatibility
* [x] Coordinates are clamped to the current valid area on restore
* [x] Main screen and alternate screen saved state are not incorrectly mixed

---

## 9. Erase Operations

> Status: 11 / 23 items clearly implemented; 12 incomplete or unverified.

### 9.1 ED — Erase in Display

* [x] `CSI 0 J` — Cursor to end of screen
* [x] `CSI 1 J` — Beginning of screen to cursor
* [x] `CSI 2 J` — Erase entire screen
* [ ] `CSI 3 J` — Clear scrollback history
* [x] `CSI J` is equivalent to default parameter 0
* [x] Erase range correctly includes or excludes the cursor cell
* [x] Erase uses current erase attributes
* [ ] Erase does not unconditionally move the cursor
* [ ] Erasing a wide character does not leave orphaned continuation cells

### 9.2 EL — Erase in Line

* [x] `CSI 0 K` — Cursor to end of line
* [x] `CSI 1 K` — Beginning of line to cursor
* [x] `CSI 2 K` — Erase entire line
* [x] `CSI K` is equivalent to default parameter 0
* [ ] Erase range is correct under left/right margin mode
* [x] Erase does not unconditionally change cursor position

### 9.3 Selective Erase

* [ ] `CSI ? 0 J` — DECSED
* [ ] `CSI ? 1 J` — DECSED
* [ ] `CSI ? 2 J` — DECSED
* [ ] `CSI ? 0 K` — DECSEL
* [ ] `CSI ? 1 K` — DECSEL
* [ ] `CSI ? 2 K` — DECSEL
* [ ] Protected cells are not erased by selective erase
* [ ] Non-protected cells are erased normally

---

## 10. Character Insertion, Deletion and Repetition

> Status: 13 / 26 items clearly implemented; 13 incomplete or unverified.

* [x] `CSI Ps @` — ICH
* [x] `CSI Ps P` — DCH
* [x] `CSI Ps X` — ECH
* [ ] `CSI Ps b` — REP

### 10.1 ICH

* [x] Default insert count is 1
* [x] Subsequent content on the current line is shifted right
* [x] Content beyond the right margin is discarded
* [ ] Newly inserted cells use the current erase attributes
* [ ] Operation occurs only within the valid left/right margins
* [ ] Wide characters are not split in half

### 10.2 DCH

* [x] Default delete count is 1
* [x] Content on the right is shifted left
* [ ] End of line is filled with erase attributes
* [ ] Operation occurs only within the valid left/right margins
* [ ] Deleting any part of a wide character fully cleans up that character

### 10.3 ECH

* [x] Default erase count is 1
* [x] Does not shift subsequent characters
* [x] Does not change cursor position
* [x] Uses current erase attributes
* [x] Does not exceed the valid right margin

### 10.4 REP

* [ ] Default repeat count is 1
* [ ] Repeats the most recently printed graphic character
* [ ] Does not repeat control characters
* [ ] Safely ignored when no previous graphic character exists
* [ ] Repeat behavior is consistent for wide characters
* [ ] Repeat process obeys autowrap rules

---

## 11. Line Operations and Scrolling

> Status: 19 / 23 items clearly implemented; 4 incomplete or unverified.

### 11.1 Line Insertion and Deletion

* [x] `CSI Ps L` — IL
* [x] `CSI Ps M` — DL
* [x] Default count is 1
* [x] Behaves correctly when cursor is outside the scrolling region
* [x] IL only affects from current cursor row to bottom margin
* [x] DL only affects from current cursor row to bottom margin
* [ ] Newly created rows use current erase attributes
* [ ] Rectangular area handling is correct under left/right margin mode

### 11.2 Explicit Scrolling

* [x] `CSI Ps S` — SU
* [x] `CSI Ps T` — SD
* [x] Default count is 1
* [x] Scrolls only the current scrolling region
* [x] Correctly truncates when scroll amount exceeds region height
* [ ] Upward scroll on main screen can enter scrollback
* [x] Alternate screen scrolling does not pollute main screen history

### 11.3 IND, NEL and RI

* [x] `ESC D` — IND
* [x] `ESC E` — NEL
* [x] `ESC M` — RI
* [x] IND triggers upward scroll at bottom margin
* [x] RI triggers downward scroll at top margin
* [x] NEL performs vertical movement and returns to the valid left margin
* [ ] IND does not unconditionally perform CR
* [x] RI does not exceed the scrolling region

---

## 12. Scrolling Region

> Status: 10 / 20 items clearly implemented; 10 incomplete or unverified.

### 12.1 DECSTBM

* [x] `CSI top;bottom r`
* [x] `CSI r` restores the full vertical region
* [x] Parameters use 1-based row numbers
* [x] top defaults to first row when omitted
* [x] bottom defaults to last row when omitted
* [x] Setting succeeds when top < bottom
* [x] Illegal regions are ignored per compatibility rules
* [x] Cursor moves to Home after setting
* [ ] Home is the top-left of the scrolling region when Origin Mode is on
* [x] Home is the top-left of the screen when Origin Mode is off

### 12.2 Left/Right Margins

* [ ] `CSI ? 69 h` — Enable DECLRMM
* [ ] `CSI ? 69 l` — Disable DECLRMM
* [ ] `CSI left;right s` — DECSLRM
* [ ] Restores full horizontal region when no parameters given
* [ ] Left/right parameters use 1-based column numbers
* [ ] Setting succeeds when left < right
* [ ] Cursor moves to Home after setting left/right margins
* [ ] Full width is restored after disabling DECLRMM
* [x] `CSI s` still works as save cursor when DECLRMM is off
* [ ] Left/right margins affect insertion, deletion, scrolling and cursor positioning

---

## 13. Horizontal Tabs

> Status: 2 / 16 items clearly implemented; 14 incomplete or unverified.

* [ ] HT — Move to next tab stop
* [ ] `ESC H` — HTS, set tab stop at current column
* [ ] `CSI Ps g` — TBC
* [ ] `CSI Ps I` — CHT
* [ ] `CSI Ps Z` — CBT

### 13.1 TBC

* [ ] `CSI 0 g` — Clear tab stop at current column
* [ ] `CSI 3 g` — Clear all tab stops
* [ ] `CSI g` uses default parameter 0
* [ ] Unknown parameters are safely ignored

### 13.2 Tab Movement

* [x] Default tab stops are set every 8 columns
* [x] HT moves to the valid right margin when no further tab stop exists
* [ ] CHT defaults to moving 1 tab stop
* [ ] CBT defaults to moving back 1 tab stop
* [ ] Tab movement does not cross the valid left/right margins
* [ ] Tab movement clears pending wrap
* [ ] Default tab stops are restored after RIS

---

## 14. Autowrap

> Status: 2 / 14 items clearly implemented; 12 incomplete or unverified.

* [ ] `CSI ? 7 h` — Enable DECAWM
* [ ] `CSI ? 7 l` — Disable DECAWM
* [ ] Writing to the last column enters pending wrap
* [ ] Does not unconditionally wrap immediately when writing to the last column
* [ ] The next printable character triggers the actual line wrap
* [ ] Soft-wrapped lines are correctly marked
* [ ] CR clears pending wrap
* [ ] BS clears or correctly handles pending wrap
* [ ] CUP/HVP clears pending wrap
* [ ] Positioning operations such as CHA/VPA clear pending wrap
* [ ] EL/ED handling of pending wrap is consistent
* [ ] The last column is overwritten when autowrap is off
* [x] Wide characters that cannot fully fit in the last column behave correctly
* [x] Scrolling is correct when autowrap triggers at the bottom of the scrolling region

---

## 15. Insert Mode and Line Feed Mode

> Status: 0 / 10 items clearly implemented; 10 incomplete or unverified.

### 15.1 IRM

* [ ] `CSI 4 h` — Enable Insert Mode
* [ ] `CSI 4 l` — Disable Insert Mode
* [ ] Printing characters in Insert Mode performs insertion first
* [ ] Insertion is limited to the current valid horizontal area
* [ ] Wide character insertion does not break cell consistency

### 15.2 LNM

* [ ] `CSI 20 h` — Enable Line Feed/New Line Mode
* [ ] `CSI 20 l` — Disable Line Feed/New Line Mode
* [ ] LF performs CR+LF semantics when LNM is on
* [ ] LF only performs vertical movement when LNM is off
* [ ] NEL behavior is not affected by erroneous duplicate CR

---

## 16. SGR Character Attributes

> Status: 51 / 71 items clearly implemented; 20 incomplete or unverified.

### 16.1 Reset and Intensity

* [x] `0` — Reset
* [x] `1` — Bold
* [x] `2` — Faint/Dim
* [x] `22` — Normal intensity
* [x] `1` and `2` can be independent or combined per target behavior
* [x] `22` clears both Bold and Faint

### 16.2 Font Style

* [x] `3` — Italic
* [x] `23` — Italic off
* [x] `9` — Strikethrough
* [x] `29` — Strikethrough off
* [ ] `53` — Overline
* [ ] `55` — Overline off

### 16.3 Underline

* [x] `4` — Single underline
* [ ] `21` — Double underline or compatibility handling
* [x] `24` — Underline off
* [ ] `4:0` — No underline
* [ ] `4:1` — Single underline
* [ ] `4:2` — Double underline
* [ ] `4:3` — Curly underline
* [ ] `4:4` — Dotted underline
* [ ] `4:5` — Dashed underline
* [x] Unknown underline styles degrade safely

### 16.4 Blink

* [x] `5` — Slow blink
* [ ] `6` — Rapid blink
* [x] `25` — Blink off

### 16.5 Inverse and Conceal

* [x] `7` — Inverse
* [x] `27` — Inverse off
* [ ] `8` — Conceal/Invisible
* [ ] `28` — Reveal

### 16.6 Basic Foreground Colors

* [x] `30` — Black
* [x] `31` — Red
* [x] `32` — Green
* [x] `33` — Yellow
* [x] `34` — Blue
* [x] `35` — Magenta
* [x] `36` — Cyan
* [x] `37` — White
* [x] `39` — Default foreground

### 16.7 Basic Background Colors

* [x] `40` — Black
* [x] `41` — Red
* [x] `42` — Green
* [x] `43` — Yellow
* [x] `44` — Blue
* [x] `45` — Magenta
* [x] `46` — Cyan
* [x] `47` — White
* [x] `49` — Default background

### 16.8 Bright Colors

* [x] `90..97` — Bright foreground
* [x] `100..107` — Bright background

### 16.9 256 Colors

* [x] `38;5;index` — Foreground
* [x] `48;5;index` — Background
* [ ] `58;5;index` — Underline color
* [x] index range limited to `0..255`
* [x] Out-of-range values are safely ignored or truncated
* [x] Incomplete sequences do not corrupt subsequent SGR

### 16.10 True Color

* [x] `38;2;R;G;B`
* [x] `48;2;R;G;B`
* [ ] `58;2;R;G;B`
* [ ] `38:2::R:G:B`
* [ ] `48:2::R:G:B`
* [ ] `58:2::R:G:B`
* [x] RGB components limited to `0..255`
* [ ] Supports colon form with empty colorspace field
* [x] Incomplete RGB parameters are safely ignored

### 16.11 Underline Color Reset

* [ ] `59` — Default underline color

### 16.12 SGR Combinations

* [x] Multiple SGR parameters are applied in order
* [x] `CSI m` is equivalent to `CSI 0 m`
* [x] Subsequent parameters continue to take effect after reset
* [x] E.g. `CSI 0;1;31 m` produces the correct result
* [x] Unknown SGR parameters do not reset all attributes
* [ ] Semicolon and colon formats can appear in the same CSI

---

## 17. DEC Private Modes

> Status: 6 / 41 items clearly implemented; 35 incomplete or unverified.

### 17.1 Cursor and Display

* [ ] `?5` — DECSCNM, reverse screen
* [ ] `?6` — DECOM, Origin Mode
* [ ] `?7` — DECAWM, autowrap
* [ ] `?12` — Cursor blink
* [ ] `?25` — Cursor visibility
* [ ] `?45` — Reverse Wraparound

### 17.2 Cursor Keys and Keypad

* [ ] `?1` — DECCKM, Application Cursor Keys
* [ ] `?66` — Application Keypad
* [ ] `ESC =` and `ESC >` can toggle keypad mode

### 17.3 Column Mode

* [ ] `?3` — 80/132 column mode
* [ ] `?40` — Allow 80/132 switching
* [ ] Can report or safely ignore when 132-column mode is unsupported
* [ ] Screen clearing behavior on column mode switch matches target compatibility

### 17.4 Alternate Screen

* [ ] `?47 h/l`
* [ ] `?1047 h/l`
* [ ] `?1048 h/l`
* [x] `?1049 h/l`
* [ ] `?1048 h` saves cursor
* [ ] `?1048 l` restores cursor
* [x] `?1049 h` saves cursor and enters alternate screen
* [x] `?1049 l` returns to main screen and restores cursor
* [x] Alternate screen does not pollute main screen content
* [x] Main screen scrollback is not overwritten by alternate screen
* [x] Repeated entering or exiting of alternate screen is stable

### 17.5 Left/Right Margins

* [ ] `?69 h/l` — DECLRMM

### 17.6 Mouse Modes

* [ ] `?9` — X10 Mouse
* [ ] `?1000` — Normal Tracking
* [ ] `?1002` — Button Event Tracking
* [ ] `?1003` — Any Event Tracking
* [ ] `?1005` — UTF-8 Mouse
* [ ] `?1006` — SGR Mouse
* [ ] `?1015` — urxvt Mouse
* [ ] `?1016` — SGR Pixel Mouse

### 17.7 Other Modern Modes

* [ ] `?1004` — Focus Reporting
* [ ] `?2004` — Bracketed Paste
* [ ] `?2026` — Synchronized Output

### 17.8 Multi-Mode Parameters

* [ ] `CSI ? 1;25;1006 h` can set multiple modes at once
* [ ] `CSI ? 1;25;1006 l` can clear multiple modes at once
* [ ] Unknown modes do not affect known modes
* [ ] Multiple modes are processed in order
* [ ] Querying an unknown mode returns unknown

---

## 18. Standard Modes

> Status: 0 / 7 items clearly implemented; 7 incomplete or unverified.

* [ ] `CSI Ps h` — SM
* [ ] `CSI Ps l` — RM
* [ ] `CSI 4 h/l` — IRM
* [ ] `CSI 20 h/l` — LNM
* [ ] Supports setting multiple modes at once
* [ ] Unknown standard modes are safely ignored
* [ ] Standard mode state is separate from DEC Private Mode state

---

## 19. Cursor Style

> Status: 9 / 10 items clearly implemented; 1 incomplete or unverified.

* [x] `CSI Ps SP q` — DECSCUSR
* [x] Correctly recognizes the Intermediate byte `SP`
* [ ] `0` — Default blinking block
* [x] `1` — Blinking block
* [x] `2` — Steady block
* [x] `3` — Blinking underline
* [x] `4` — Steady underline
* [x] `5` — Blinking bar
* [x] `6` — Steady bar
* [x] Unknown values use default style or are safely ignored

---

## 20. Soft Reset and Hard Reset

> Status: 3 / 27 items clearly implemented; 24 incomplete or unverified.

### 20.1 RIS

* [x] `ESC c` — RIS
* [ ] Reset character attributes
* [x] Reset cursor position
* [ ] Reset scrolling region
* [ ] Reset left/right margins
* [ ] Reset Origin Mode
* [ ] Reset autowrap mode
* [ ] Reset insert mode
* [ ] Reset character sets
* [ ] Reset tab stops
* [ ] Reset mouse modes
* [ ] Reset bracketed paste
* [ ] Reset focus reporting
* [ ] Reset synchronized output
* [ ] Exit alternate screen
* [ ] Clear pending wrap
* [x] Clear incomplete control sequence state

### 20.2 DECSTR

* [ ] `CSI ! p` — DECSTR
* [ ] Correctly recognizes the Intermediate byte `!`
* [ ] Resets selected modes
* [ ] Resets character attributes
* [ ] Resets cursor visibility
* [ ] Resets autowrap
* [ ] Resets Origin Mode
* [ ] Resets insert mode
* [ ] Is not erroneously equivalent to RIS
* [ ] Does not unconditionally clear the entire terminal history

---

## 21. Device Status Reports

> Status: 0 / 22 items clearly implemented; 22 incomplete or unverified.

### 21.1 DSR

* [ ] `CSI 5 n` — Request device status
* [ ] Responds with `CSI 0 n`
* [ ] `CSI 6 n` — Request cursor position
* [ ] Responds with `CSI row;col R`
* [ ] Response coordinates use 1-based
* [ ] Response coordinates match target compatibility behavior under Origin Mode
* [ ] `CSI ? 6 n` — DEC Private CPR
* [ ] Private CPR response format is correct

### 21.2 Primary Device Attributes

* [ ] `CSI c`
* [ ] `CSI 0 c`
* [ ] Returns Primary DA
* [ ] Return value only declares actually supported capabilities
* [ ] Does not return unimplemented advanced feature flags

### 21.3 Secondary Device Attributes

* [ ] `CSI > c`
* [ ] `CSI > 0 c`
* [ ] Returns `CSI > Pp;Pv;Pc c`
* [ ] Terminal type, version and ROM field format is stable

### 21.4 Tertiary Device Attributes

* [ ] `CSI = c`
* [ ] Optionally supports Tertiary DA
* [ ] Safely ignored when unsupported

### 21.5 DECID

* [ ] `ESC Z`
* [ ] Response is compatible with Primary DA

---

## 22. Mode Queries

> Status: 0 / 12 items clearly implemented; 12 incomplete or unverified.

### 22.1 DECRQM

* [ ] `CSI Ps $ p`
* [ ] `CSI ? Ps $ p`
* [ ] Correctly recognizes Intermediate `$`
* [ ] Queries standard modes
* [ ] Queries DEC Private Modes

### 22.2 DECRPM Response

* [ ] `CSI Ps;1 $ y` — Set
* [ ] `CSI Ps;2 $ y` — Reset
* [ ] `CSI Ps;3 $ y` — Permanently Set
* [ ] `CSI Ps;4 $ y` — Permanently Reset
* [ ] `CSI Ps;0 $ y` — Unknown
* [ ] Private Mode response preserves `?`
* [ ] Querying an unknown mode does not return an error state

---

## 23. Window Operations

> Status: 0 / 19 items clearly implemented; 19 incomplete or unverified.

* [ ] `CSI Ps t` basic parsing
* [ ] Supports multi-parameter window operations
* [ ] Unauthorized window operations can be safely ignored

### 23.1 Common Queries

* [ ] `CSI 11 t` — Query window state
* [ ] `CSI 13 t` — Query window position
* [ ] `CSI 14 t` — Query text area pixel size
* [ ] `CSI 16 t` — Query character cell pixel size
* [ ] `CSI 18 t` — Query character area row/column count
* [ ] `CSI 19 t` — Query screen character size
* [ ] `CSI 20 t` — Query icon title
* [ ] `CSI 21 t` — Query window title

### 23.2 Common Controls

* [ ] `CSI 1 t` — Restore
* [ ] `CSI 2 t` — Minimize
* [ ] `CSI 3;x;y t` — Move
* [ ] `CSI 4;h;w t` — Resize Pixels
* [ ] `CSI 8;rows;cols t` — Resize Characters
* [ ] Permission control for move and resize commands
* [ ] Query response format is correct
* [ ] Does not return incorrect row/column or pixel dimensions

---

## 24. OSC Basic Parsing

> Status: 8 / 78 items clearly implemented; 70 incomplete or unverified.

### 24.1 OSC Boundaries

* [x] `ESC ]` begins OSC
* [ ] 8-bit OSC `0x9D`
* [x] BEL terminates
* [x] ST `ESC \` terminates
* [ ] 8-bit ST `0x9C` terminates
* [x] OSC content can span input fragments
* [x] `ESC` and the following `\` can span input fragments
* [ ] Enters discard state when OSC exceeds length limit
* [ ] Oversized OSC can eventually recover at the terminator
* [x] OSC content is not displayed as plain text
* [x] Unknown OSC numbers can be safely ignored

### 24.2 Titles

* [ ] `OSC 0 ; title ST`
* [ ] `OSC 1 ; icon-name ST`
* [ ] `OSC 2 ; window-title ST`
* [ ] Supports BEL termination
* [ ] Supports ST termination
* [ ] Title length limit
* [ ] Control characters in titles are filtered
* [ ] Empty titles are handled correctly

### 24.3 Palette

* [ ] `OSC 4 ; index ; color ST`
* [ ] Can set multiple palette entries at once
* [ ] `OSC 4 ; index ; ? ST` queries color
* [ ] Supports `rgb:RR/GG/BB`
* [ ] Supports shorter RGB component formats
* [ ] Illegal color formats are safely ignored
* [ ] Palette index is limited to the valid range

### 24.4 Default Colors

* [ ] `OSC 10 ; color ST` — Default foreground
* [ ] `OSC 11 ; color ST` — Default background
* [ ] `OSC 12 ; color ST` — Cursor color
* [ ] `OSC 10 ; ? ST` — Query default foreground
* [ ] `OSC 11 ; ? ST` — Query default background
* [ ] `OSC 12 ; ? ST` — Query cursor color
* [ ] `OSC 110 ST` — Reset default foreground
* [ ] `OSC 111 ST` — Reset default background
* [ ] `OSC 112 ST` — Reset cursor color
* [ ] `OSC 104 ST` — Reset palette
* [ ] `OSC 104 ; index ST` — Reset specified color

### 24.5 Current Working Directory

* [ ] `OSC 7 ; file://host/path ST`
* [ ] URI parsing is correct
* [ ] Percent-encoding is handled correctly
* [ ] Host and path are separated
* [ ] Illegal URIs are safely ignored
* [ ] Untrusted paths do not directly trigger file operations

### 24.6 Hyperlinks

* [ ] `OSC 8 ; params ; URI ST`
* [ ] `OSC 8 ; ; ST` closes hyperlink
* [ ] Supports `id=` parameter
* [ ] Empty URI closes the current hyperlink
* [ ] Hyperlink state can persist across plain text
* [ ] SGR Reset does not erroneously close the hyperlink
* [ ] RIS closes the current hyperlink
* [ ] URI length is limited
* [ ] Illegal URIs do not cause parse desynchronization

### 24.7 Clipboard

* [ ] `OSC 52 ; selection ; base64 ST`
* [ ] Supports common selection fields
* [ ] Empty selection behavior is well-defined
* [ ] Base64 decoding is strict
* [ ] Payload length limit
* [ ] Decoded length limit
* [ ] Writing clipboard is permission-controlled
* [ ] Querying clipboard is permission-controlled
* [ ] Remote sessions cannot silently read the clipboard by default
* [ ] Illegal Base64 is safely ignored

### 24.8 Shell Integration

* [ ] `OSC 133 ; A ST`
* [ ] `OSC 133 ; B ST`
* [ ] `OSC 133 ; C ST`
* [ ] `OSC 133 ; D ST`
* [ ] `OSC 133 ; D ; exit-code ST`
* [ ] Unknown OSC 133 sub-commands are safely ignored

### 24.9 Notification Extensions

* [ ] OSC 9
* [ ] OSC 99
* [ ] OSC 777
* [ ] Notification functionality is permission-controlled
* [ ] Notification title and body length are limited

### 24.10 OSC 1337

* [ ] Can fully recognize and consume OSC 1337
* [x] Payload is not displayed when unsupported
* [ ] File transfer functionality is permission-controlled
* [ ] Inline image data size is limited
* [ ] Illegal parameters do not cause parse desynchronization

---

## 25. DCS Basic Parsing

> Status: 0 / 42 items clearly implemented; 42 incomplete or unverified.

### 25.1 DCS Boundaries

* [ ] `ESC P` begins DCS
* [ ] 8-bit DCS `0x90`
* [ ] Supports parameter bytes
* [ ] Supports Private Marker
* [ ] Supports Intermediate
* [ ] Supports Final byte
* [ ] Supports payload
* [ ] ST terminates
* [ ] 8-bit ST terminates
* [ ] DCS can span input fragments
* [ ] Oversized DCS can enter discard state
* [ ] Unsupported DCS can be fully consumed then ignored
* [ ] DCS payload is not displayed as plain text

### 25.2 DECRQSS

* [ ] `DCS $ q Pt ST`
* [ ] Request SGR state
* [ ] Request scrolling region state
* [ ] Request left/right margin state
* [ ] Success response format is correct
* [ ] Failure response format is correct
* [ ] Response content is escaped and length-limited

### 25.3 XTGETTCAP

* [ ] `DCS + q ... ST`
* [ ] Can parse hex-encoded capability names
* [ ] Can query multiple capabilities
* [ ] Success response `DCS 1 + r ... ST`
* [ ] Failure response `DCS 0 + r ... ST`
* [ ] Return values use correct hex encoding
* [ ] Does not declare actually unsupported capabilities
* [ ] Request length is limited

### 25.4 Sixel

* [ ] Can recognize Sixel DCS
* [ ] Fully consumes to ST when unsupported
* [ ] Payload is not displayed as text when unsupported
* [ ] Optionally parses repeat introducer
* [ ] Optionally parses raster attributes
* [ ] Optionally parses color register
* [ ] Optionally parses carriage return
* [ ] Optionally parses next line
* [ ] Image size limit
* [ ] Palette count limit
* [ ] Memory limit after decoding

### 25.5 Other DCS

* [ ] DECUDK can be fully consumed
* [ ] ReGIS can be fully consumed
* [ ] Unknown DCS does not affect the next sequence

---

## 26. APC, PM and SOS

> Status: 0 / 18 items clearly implemented; 18 incomplete or unverified.

### 26.1 Basic Parsing

* [ ] APC: `ESC _ ... ST`
* [ ] PM: `ESC ^ ... ST`
* [ ] SOS: `ESC X ... ST`
* [ ] Supports 8-bit introducer
* [ ] Supports ST termination
* [ ] Content can span input fragments
* [ ] Content is not displayed as plain text
* [ ] Oversized content enters discard state
* [ ] Unknown protocols are safely ignored

### 26.2 Kitty Graphics

* [ ] Can recognize APC `G`
* [ ] Fully consumes to ST when unsupported
* [ ] Control parameters and payload can be separated
* [ ] Base64 payload length is limited
* [ ] Decoded image size is limited
* [ ] Chunked transmission can be correctly associated
* [ ] Delete image commands are handled safely
* [ ] File path transmission is permission-controlled
* [ ] Illegal image commands do not break the VT parser

---

## 27. String Sequence Interruption and Termination

> Status: 13 / 13 items clearly implemented; 0 incomplete or unverified.

* [x] BEL terminates OSC normally
* [x] ST terminates OSC normally
* [x] ST terminates DCS normally
* [x] ST terminates APC normally
* [x] ST terminates PM normally
* [x] ST terminates SOS normally
* [x] The ESC and backslash of `ESC \` can be fragmented
* [x] CAN in a string cancels the current string
* [x] SUB in a string cancels the current string
* [x] Non-ST ESC within a string is handled per target compatibility behavior
* [x] The next sequence can be parsed immediately after string cancellation
* [x] Oversized strings do not grow memory indefinitely
* [x] Terminators are still recognized while an oversized string is being discarded

---

## 28. Mouse Protocol Output

> Status: 0 / 34 items clearly implemented; 34 incomplete or unverified.

### 28.1 Mode Priority

* [ ] X10
* [ ] Normal Tracking
* [ ] Button Event Tracking
* [ ] Any Event Tracking
* [ ] Correct priority is used when multiple modes are enabled simultaneously
* [ ] Corresponding events are no longer sent after disabling a mode

### 28.2 X10 Encoding

* [ ] `CSI M Cb Cx Cy`
* [ ] Coordinates are encoded with the protocol offset
* [ ] Behavior is well-defined when coordinates exceed range
* [ ] X10 only reports press events
* [ ] Modifier bits are encoded correctly

### 28.3 SGR Mouse

* [ ] Press: `CSI < Cb;Cx;Cy M`
* [ ] Release: `CSI < Cb;Cx;Cy m`
* [ ] Coordinates use 1-based
* [ ] Left button encoding correct
* [ ] Middle button encoding correct
* [ ] Right button encoding correct
* [ ] Release encoding correct
* [ ] Scroll up encoding correct
* [ ] Scroll down encoding correct
* [ ] Horizontal scroll optionally supported
* [ ] Shift bit correct
* [ ] Alt/Meta bit correct
* [ ] Ctrl bit correct
* [ ] Motion bit correct
* [ ] Drag and Move are correctly distinguished

### 28.4 Pixel Mouse

* [ ] `?1016` enables pixel coordinates
* [ ] Coordinates use pixels rather than cells
* [ ] Coordinate origin and 1-based rules are correct
* [ ] Switching with normal SGR Mouse is correct

### 28.5 Focus Reporting

* [ ] Sends `CSI I` on focus gained
* [ ] Sends `CSI O` on focus lost
* [ ] Only sent when `?1004` is enabled
* [ ] Duplicate focus events can be deduplicated per policy

---

## 29. Bracketed Paste

> Status: 0 / 10 items clearly implemented; 10 incomplete or unverified.

* [ ] `CSI ? 2004 h`
* [ ] `CSI ? 2004 l`
* [ ] Paste start sends `ESC [ 200 ~`
* [ ] Paste end sends `ESC [ 201 ~`
* [ ] Wrapped only when mode is enabled
* [ ] Empty paste behaves correctly
* [ ] Multi-line text preserves expected line breaks
* [ ] Large text does not omit the end marker
* [ ] ESC sequences in pasted content are sent as data
* [ ] Normal paste behavior is restored after disabling the mode

---

## 30. Synchronized Output

> Status: 0 / 9 items clearly implemented; 9 incomplete or unverified.

* [ ] `CSI ? 2026 h`
* [ ] `CSI ? 2026 l`
* [ ] Supports nested enabling or explicitly forbids nesting
* [ ] Multiple enables do not prematurely end the synchronized state
* [ ] Synchronized state ends after corresponding number of disables
* [ ] RIS clears the synchronized state
* [ ] PTY close clears the synchronized state
* [ ] Safe recovery mechanism exists when the application does not close the mode
* [ ] Querying `?2026` returns the correct state

---

## 31. Keyboard Mode Related Protocols

> Status: 0 / 66 items clearly implemented; 66 incomplete or unverified.

### 31.1 Application Cursor Keys

* [ ] Normal Up: `ESC [ A`
* [ ] Normal Down: `ESC [ B`
* [ ] Normal Right: `ESC [ C`
* [ ] Normal Left: `ESC [ D`
* [ ] Application Up: `ESC O A`
* [ ] Application Down: `ESC O B`
* [ ] Application Right: `ESC O C`
* [ ] Application Left: `ESC O D`
* [ ] `?1 h/l` switches correctly

### 31.2 Home and End

* [ ] Normal Home encoding
* [ ] Normal End encoding
* [ ] Application Home encoding
* [ ] Application End encoding
* [ ] Consistent with terminfo declarations

### 31.3 Keypad

* [ ] Numeric Keypad mode
* [ ] Application Keypad mode
* [ ] `ESC =` switches to Application
* [ ] `ESC >` switches to Numeric
* [ ] Keypad Enter
* [ ] Keypad digits
* [ ] Keypad operators

### 31.4 Function Keys

* [ ] F1
* [ ] F2
* [ ] F3
* [ ] F4
* [ ] F5
* [ ] F6
* [ ] F7
* [ ] F8
* [ ] F9
* [ ] F10
* [ ] F11
* [ ] F12
* [ ] Shift modifier
* [ ] Alt modifier
* [ ] Ctrl modifier
* [ ] Multiple modifier key combinations

### 31.5 Editing Keys

* [ ] Insert
* [ ] Delete
* [ ] Page Up
* [ ] Page Down
* [ ] Home
* [ ] End
* [ ] Backspace
* [ ] Enter
* [ ] Tab
* [ ] Shift+Tab
* [ ] Escape

### 31.6 ModifyOtherKeys

* [ ] Can recognize xterm ModifyOtherKeys configuration sequences
* [ ] Level 1
* [ ] Level 2
* [ ] Can be disabled
* [ ] Encoding is correct when modifying ordinary characters
* [ ] Traditional encoding is preserved when not enabled

### 31.7 Kitty Keyboard Protocol

* [ ] Can recognize Kitty Keyboard Protocol enable sequence
* [ ] Can recognize disable sequence
* [ ] Supports progressive enhancement flags
* [ ] Supports key press
* [ ] Supports key repeat
* [ ] Supports key release
* [ ] Supports Unicode code point
* [ ] Supports modifier bitmask
* [ ] Supports functional key code
* [ ] Supports associated text
* [ ] Supports push/pop keyboard mode
* [ ] Does not send incorrectly formatted sequences when unsupported

---

## 32. Rectangular Area Operations

> Status: 0 / 11 items clearly implemented; 11 incomplete or unverified.

* [ ] DECFRA — Fill Rectangular Area
* [ ] DECERA — Erase Rectangular Area
* [ ] DECSERA — Selective Erase Rectangular Area
* [ ] DECCRA — Copy Rectangular Area
* [ ] DECCARA — Change Attributes in Rectangular Area
* [ ] DECRARA — Reverse Attributes in Rectangular Area
* [ ] Rectangle coordinates use correct 1-based semantics
* [ ] Illegal rectangle ranges are safely ignored
* [ ] Rectangle range is clipped to the valid screen
* [ ] Protected cells are preserved during selective erase
* [ ] Rectangle operations do not split wide characters

---

## 33. Character Protection Attribute

> Status: 0 / 6 items clearly implemented; 6 incomplete or unverified.

* [ ] DECSCA sets character protection state
* [ ] Subsequently written characters inherit the protection attribute
* [ ] Normal ED/EL can erase protected cells
* [ ] DECSED/DECSEL do not erase protected cells
* [ ] Effect of SGR Reset on protection attribute matches target behavior
* [ ] RIS clears the protection state

---

## 34. Terminal Status Report Strings

> Status: 0 / 8 items clearly implemented; 8 incomplete or unverified.

### 34.1 DECRQSS Query Items

* [ ] SGR
* [ ] DECSTBM
* [ ] DECSLRM
* [ ] DECSCUSR
* [ ] DECSCA
* [ ] Querying a supported item returns success
* [ ] Querying an unsupported item returns failure
* [ ] Response sequences can be correctly parsed by other VT parsers

---

## 35. Error Recovery

> Status: 17 / 17 items clearly implemented; 0 incomplete or unverified.

* [x] Unknown ESC sequences do not crash
* [x] Unknown CSI sequences do not crash
* [x] Unknown OSC sequences do not crash
* [x] Unknown DCS sequences do not crash
* [x] Unknown APC sequences do not crash
* [x] CSI parameter overflow does not crash
* [x] Excessive parameter count does not allocate indefinitely
* [x] Excessive Intermediates do not allocate indefinitely
* [x] Unterminated OSC does not allocate indefinitely
* [x] Unterminated DCS does not allocate indefinitely
* [x] Unterminated APC does not allocate indefinitely
* [x] Can recover when illegal characters enter CSI
* [x] CAN can recover from any sequence state
* [x] SUB can recover from any sequence state
* [x] ESC can interrupt CSI and start a new ESC
* [x] Normal text displays correctly after a sequence is ignored
* [x] Consecutive illegal sequences do not permanently desynchronize the parser

---

## 36. Protocol Security Limits

> Status: 17 / 22 items clearly implemented; 5 incomplete or unverified.

* [x] Maximum CSI parameter count
* [x] Maximum CSI sub-parameter count
* [x] Maximum parameter value
* [x] Maximum Intermediate count
* [x] Maximum OSC length
* [x] Maximum DCS length
* [x] Maximum APC length
* [x] Maximum PM length
* [x] Maximum SOS length
* [x] Maximum title length
* [x] Maximum hyperlink URI length
* [x] Maximum OSC 52 Base64 length
* [x] Maximum clipboard decoded length
* [x] Maximum image transfer length
* [x] Maximum image decoded size
* [x] No further memory allocation after exceeding the limit
* [x] Terminator can still be scanned after exceeding the limit
* [ ] OSC 52 read is restricted by default
* [ ] OSC 52 write is restricted by default
* [ ] File transfer protocol is restricted by default
* [ ] Desktop notifications are restricted by default
* [ ] Window move and resize are restricted by default

---

## 37. Minimum Modern Compatibility Set

> Status: 21 / 56 items clearly implemented; 35 incomplete or unverified.

### 37.1 Shell Basic Compatibility

* [ ] C0
* [ ] ESC
* [x] CSI
* [x] CUP
* [x] CUU/CUD/CUF/CUB
* [x] ED/EL
* [x] SGR
* [x] CR/LF/BS/HT
* [x] DECSTBM
* [ ] DECAWM
* [x] DECSC/DECRC
* [ ] DSR
* [ ] DA1

### 37.2 Vim/Neovim Compatibility

* [ ] `?1`
* [ ] `?6`
* [ ] `?7`
* [ ] `?25`
* [x] `?1049`
* [ ] `?2004`
* [ ] `?1004`
* [ ] `?1006`
* [ ] `?2026`
* [x] ICH
* [x] DCH
* [x] IL
* [x] DL
* [x] ECH
* [x] SU
* [x] SD
* [x] DECSCUSR
* [x] 256 colors
* [x] True Color
* [ ] Curly underline
* [ ] Underline color
* [ ] OSC 8
* [ ] DA1/DA2
* [ ] CPR

### 37.3 tmux Compatibility

* [ ] XTGETTCAP
* [ ] DECRQSS
* [ ] DECRQM
* [ ] DA1
* [ ] DA2
* [ ] OSC 8
* [ ] OSC 52
* [ ] Bracketed Paste
* [ ] Focus Reporting
* [ ] SGR Mouse
* [ ] Application Cursor
* [ ] Application Keypad
* [x] Alternate Screen

### 37.4 Modern Shell Integration

* [ ] OSC 7
* [ ] OSC 8
* [ ] OSC 133
* [ ] OSC 9/99/777 optional
* [x] OSC 1337 can be safely ignored
* [ ] Synchronized Output

---

## 38. Sequence-Level Test Samples

> Status: 46 / 83 items clearly implemented; 37 incomplete or unverified.

### 38.1 Basic Text and Attributes

* [x] `hello`
* [x] `\x1b[31mred\x1b[0m`
* [x] `\x1b[1;3;4mstyled\x1b[0m`
* [x] `\x1b[38;5;196mindexed\x1b[0m`
* [x] `\x1b[38;2;255;128;0mtruecolor\x1b[0m`
* [ ] `\x1b[4:3mcurly\x1b[4:0m`
* [ ] `\x1b[58;2;255;0;0munderline-color\x1b[59m`

### 38.2 Cursor

* [x] `\x1b[10;20H`
* [x] `\x1b[5A`
* [x] `\x1b[5B`
* [x] `\x1b[5C`
* [x] `\x1b[5D`
* [x] `\x1b[10G`
* [x] `\x1b[10d`
* [x] `\x1b7`
* [x] `\x1b8`
* [x] `\x1b[s`
* [x] `\x1b[u`

### 38.3 Erase

* [x] `\x1b[J`
* [x] `\x1b[1J`
* [x] `\x1b[2J`
* [ ] `\x1b[3J`
* [x] `\x1b[K`
* [x] `\x1b[1K`
* [x] `\x1b[2K`
* [ ] `\x1b[?2J`
* [ ] `\x1b[?2K`

### 38.4 Scrolling Region

* [x] `\x1b[2;20r`
* [x] `\x1b[r`
* [ ] `\x1b[?6h`
* [ ] `\x1b[?6l`
* [x] `\x1bD`
* [x] `\x1bM`
* [x] `\x1b[3S`
* [x] `\x1b[3T`

### 38.5 Insertion and Deletion

* [x] `\x1b[3@`
* [x] `\x1b[3P`
* [x] `\x1b[3X`
* [x] `\x1b[3L`
* [x] `\x1b[3M`
* [ ] `A\x1b[5b`

### 38.6 Modes

* [ ] `\x1b[?1h`
* [ ] `\x1b[?1l`
* [ ] `\x1b[?7h`
* [ ] `\x1b[?7l`
* [ ] `\x1b[?25h`
* [ ] `\x1b[?25l`
* [x] `\x1b[?1049h`
* [x] `\x1b[?1049l`
* [ ] `\x1b[?2004h`
* [ ] `\x1b[?2004l`
* [ ] `\x1b[?2026h`
* [ ] `\x1b[?2026l`

### 38.7 Queries

* [ ] `\x1b[5n`
* [ ] `\x1b[6n`
* [ ] `\x1b[c`
* [ ] `\x1b[>c`
* [ ] `\x1b[?25$p`
* [ ] `\x1bP$qm\x1b\\`

### 38.8 OSC

* [ ] `\x1b]0;title\x07`
* [ ] `\x1b]2;title\x1b\\`
* [ ] `\x1b]7;file:///tmp\x1b\\`
* [ ] `\x1b]8;;https://example.com\x1b\\link\x1b]8;;\x1b\\`
* [ ] `\x1b]10;?\x1b\\`
* [ ] `\x1b]11;?\x1b\\`
* [ ] `\x1b]52;c;SGVsbG8=\x1b\\`

### 38.9 Character Sets

* [ ] `\x1b(0lqqk\x1b(B`
* [ ] `\x0exqqq\x0f`
* [ ] `\x1b#8`

### 38.10 Error Recovery

* [x] Incomplete ESC
* [x] Incomplete CSI
* [x] Incomplete OSC
* [ ] Incomplete DCS
* [x] CAN inserted within CSI
* [x] SUB inserted within CSI
* [x] New ESC inserted within CSI
* [x] Oversized parameter
* [x] Excessive parameters
* [ ] Oversized OSC
* [ ] Oversized DCS
* [x] Unknown Final
* [x] Unknown Private Marker
* [x] Unknown Intermediate combination

---

## 39. Final Acceptance

> Status: 6 / 21 items clearly implemented; 15 incomplete or unverified.

* [ ] All control sequences support arbitrary input fragmentation
* [ ] All unknown sequences can be safely ignored
* [ ] All string protocols have length limits
* [ ] All query responses conform to protocol format
* [x] Does not declare terminal capabilities not actually supported
* [ ] Screen state is correct after Neovim launch, run, and exit
* [ ] State is correct after tmux launch, split, and exit
* [ ] less, top, htop, fzf, lazygit work correctly
* [ ] Application cursor keys and normal cursor keys switch correctly
* [x] Main screen and alternate screen switch correctly
* [ ] Bracketed Paste correct
* [ ] Focus Reporting correct
* [ ] SGR Mouse correct
* [ ] True Color correct
* [ ] Curly Underline correct
* [ ] OSC 8 Hyperlink correct
* [ ] DSR, DA, DECRQM, DECRQSS responses correct
* [x] Malformed sequences do not permanently desynchronize the parser
* [x] Arbitrary byte input does not panic
* [x] Arbitrary byte input does not cause infinite loops
* [x] Arbitrary byte input does not cause unbounded memory growth
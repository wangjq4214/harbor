# VT reference: implemented behavior and deliberate limits

[Terminal core](README.md) · [Compatibility](compatibility.md) · [Security](security.md)

This is a concise navigation/reference layer over the [detailed protocol checklist](../protocol/checklist.md). It describes common implemented paths and points to their owners; it does not replace the checklist's parameter edge cases, focused tests or open acceptance items. A checked implementation item is not runtime proof for an application or transport.

## Notation and framing

- `ESC` = byte `0x1b`; `CSI` = `ESC [`; `OSC` = `ESC ]`; `DCS` = `ESC P`; `ST` = `ESC \`.
- Examples below use 7-bit introducers and UTF-8 text. The parser has opt-in 8-bit C1 handling, but `Terminal` does **not** enable it by default. UTF-8 text is not permission to reinterpret continuation bytes as C1 controls.
- `Ps` is a numeric parameter; `Pn` is usually a count. Missing, explicit zero and colon subparameters are sequence-specific, not interchangeable across all commands.
- OSC accepts BEL or ST termination. DCS/APC/PM/SOS use string framing; CAN/SUB cancel unfinished work. Incremental input may split anywhere, including within UTF-8, ESC or ST.
- Bounded consumption of an unknown string family is **safe discard**, not implementation of the feature carried inside it. See [resource limits](security.md#resource-limits-and-what-they-do-not-bound).

Owners: [parser state machine](../../crates/harbor-parser/src/core.rs), [terminal dispatcher](../../crates/harbor-terminal/src/parser/handlers.rs), [incremental tests](../../crates/harbor-terminal/src/parser/incremental_tests.rs). Detailed [framing and recovery requirements](../protocol/checklist.md#35-error-recovery) remain authoritative.

## Text, cursor and editing

The following routes are implemented in the dispatcher/screen; consult the linked checklist section for exact boundaries and tests.

| Family | Common wire forms | Contract / detail |
| --- | --- | --- |
| Basic controls | BS, HT, LF/VT/FF, CR, SO/SI | Backspace, tab, line feed, carriage return, charset invocation. BEL/ENQ are consumed without a bell/answerback host effect. [C0](../protocol/checklist.md#2-c0-control-characters) |
| Cursor movement | `CSI Pn A/B/C/D`, `E/F`, `G`, `H/f`, `d`; aliases `a/e` and HPA | Rows/columns are 1-based on the wire; active margins/origin constrain movement. [Movement](../protocol/checklist.md#7-cursor-movement) |
| Save/restore | `ESC 7/8`, `CSI s/u`, `?1048 h/l` | `CSI s` instead sets left/right margins when margin mode is enabled. [Save/restore](../protocol/checklist.md#8-save-and-restore-cursor) |
| Erase | `CSI Ps J/K`, `CSI Pn X`, selective `CSI ? Ps J/K` | Erase uses current blank/protection policy; display erase mode 3 handles scrollback. [Erase](../protocol/checklist.md#9-erase-operations) |
| Character edits | `CSI Pn @/P/b` | Insert, delete, repeat. Wide text units require whole-unit invariants. [Character operations](../protocol/checklist.md#10-character-insertion-deletion-and-repetition) |
| Lines/scroll | `CSI Pn L/M/S/T`, `ESC D/E/M` | Insert/delete lines, scroll regions, IND/NEL/RI. [Scrolling](../protocol/checklist.md#11-line-operations-and-scrolling) |
| Margins/tabs | `CSI top;bottom r`, `CSI left;right s`, `ESC H`, `CSI Ps g`, `CSI Pn I/Z` | Scroll region, left/right margins, set/clear tab stops and forward/back tab. [Margins](../protocol/checklist.md#12-scrolling-region), [tabs](../protocol/checklist.md#13-horizontal-tabs) |
| Charset/alignment | `ESC (/)/*/+` designation, `ESC N/O`, `ESC # 8` | Supported designations include DEC graphics; not every ISO-2022 charset. DECALN fills the visible grid with `E`. [Charsets](../protocol/checklist.md#5-character-set-selection) |
| Rectangles/protection | `CSI ... $ z/{/x/v/r/t`, `CSI Ps " q` | DECERA/DECSERA/DECFRA/DECCRA/DECCARA/DECRARA; DECSCA selective-erase protection. [Rectangles](../protocol/checklist.md#32-rectangular-area-operations) |
| Reset | `ESC c`, `CSI ! p` | RIS and DECSTR differ. Do not assume a soft reset clears session metadata, palette or nested synchronized-output state. [Reset](../protocol/checklist.md#20-soft-reset-and-hard-reset) |

## Rendition and modes

[SGR](../protocol/checklist.md#16-sgr-character-attributes) supports basic/bright/256/RGB colors, intensity, italic, inverse, conceal, strike, overline and modern underline style/color. Typical forms include `31`, `38;5;n`, `38;2;r;g;b`, `4:0..5`, `58:2::r:g:b`, `24`, `59` and `0` followed by `m`. Colon/semicolon color forms have their own validation. Storing an attribute such as blink does not establish a visible text-blink animation contract. Conceal does not remove source text.

The [pen implementation](../../crates/harbor-terminal/src/screen/edit/pen_state.rs), [modern SGR tests](../../crates/harbor-terminal/tests/modern_sgr_acceptance.rs) and [scoped native SGR evidence](../modern-sgr-acceptance.md) distinguish parsing, status replay and rendered output.

| Modes | Implemented purpose |
| --- | --- |
| Standard `4`, `20` (`CSI Ps h/l`) | Insert and line-feed modes. |
| Private `?1`, `?66`; `ESC =/>` | Application cursor/keypad state and traditional input encoding. |
| Private `?6`, `?7`, `?25`, `?69` | Origin, autowrap, cursor visibility, left/right margins. |
| Private `?47`, `?1047`, `?1048`, `?1049` | Alternate-buffer and cursor-save family; see [buffer concepts](concepts.md#primary-and-alternate-buffers). |
| Private `?1000`, `?1002`, `?1003` | Independent button/button-motion/any-motion tracking flags, effective priority `1003 > 1002 > 1000`. |
| Private `?1005`, `?1015`, `?1006`, `?1016` | UTF-8, urxvt, SGR cell and SGR physical-pixel mouse encoding. Effective priority `1016 > 1006 > 1015 > 1005 > legacy`. Encoding alone enables no tracking. |
| Private `?1004` | Edge-triggered focus reports `CSI I/O`; enabling alone or repeated focus observations emit nothing. |
| Private `?2004` | Bracketed paste framing around actual paste input. |
| Private `?2026` | Nested synchronized-output suppression of ordinary presentation, not suppression of parsing. Recovery presentation is host-owned. |
| `CSI Ps SP q` | Cursor shape/blink selection (DECSCUSR). |

Modes are set/reset by `h/l`; unsupported mode queries report unknown rather than fabricating support. `?9` press-only X10 tracking is not implemented; legacy `CSI M` **encoding** under supported tracking is a different capability. Legacy coordinates above 223 and UTF-8 coordinates above 2015 are suppressed, not wrapped. See [mouse reference](../protocol/checklist.md#28-mouse-protocol-output), [mouse evidence](../multi-encoding-mouse-acceptance.md), [input encoder](../../crates/harbor-terminal/src/input.rs) and [mode state](../../crates/harbor-terminal/src/screen/cursor.rs).

## Queries and replies

| Request | Current reply / restriction |
| --- | --- |
| `CSI 5 n` | `CSI 0 n` (status OK). |
| `CSI 6 n`, `CSI ? 6 n` | Standard/private CPR with current 1-based cursor coordinates respecting the implemented origin/margin rules. |
| `CSI c` or `CSI 0 c`, `ESC Z` | Primary DA / DECID: `ESC [ ? 62 ; 6 ; 17 ; 22 ; 28 c`. |
| `CSI > c` or `CSI > 0 c` | Secondary DA: `ESC [ > 1 ; 1 ; 0 c`. Fixed protocol identity, not the application build version. |
| `CSI Ps $ p`, `CSI ? Ps $ p` | DECRQM/DECRPM mode status; only the supported request shape. |
| `DCS $ q Pt ST` | DECRQSS for implemented SGR, scroll/margin, cursor-style and protection status strings; not arbitrary terminal state. |
| `DCS + q hex-names ST` | XTGETTCAP registry: `TN=xterm-256color`, `RGB=8/8/8`, lowercase `u8` and `Su` name-only booleans. Unknown/malformed names are skipped; no match/overflow produces the defined failure frame. |

Tertiary DA (`CSI = c`) is unsupported and ignored, not a hidden stronger identity. XTGETTCAP currently does **not** advertise `Co/colors` or uppercase `U8`; lowercase `u8` has a known ncurses naming collision, so do not treat it as a complete terminfo database. Multiple-name handling deliberately differs from xterm's stop-at-first-unknown behavior. See [capability registry](../../crates/harbor-terminal/src/parser/xtgettcap.rs), [DA registry](../../crates/harbor-terminal/src/parser/device_attributes.rs), [status strings](../../crates/harbor-terminal/src/parser/status_strings.rs) and [query checklist](../protocol/checklist.md#21-device-status-reports).

Replies are byte output, **not** `TerminalOutputEvent` values. On the queued live/custom-I/O path the session drains replies from `Screen` and writes them to the input endpoint. Direct `Terminal::put_bytes/process_output` only parse; they do not implement an external reply transport. A lower-level parser/screen integrator can drain `Screen::drain_replies`, but must also handle buffer transitions and every other integration obligation. The current `Terminal` facade offers no public direct reply-drain method. See [I/O reply handling](../../crates/harbor-terminal/src/io.rs).

## OSC and string families

| Family | Implemented semantics / boundary |
| --- | --- |
| OSC `0/1/2` | Validated title change/reset event. No native title mutation inside the core. |
| OSC `4/104` | Session-owned indexed palette set/query/reset. Malformed requests do not partially mutate; valid operations run in order. |
| OSC `10/11/12`, `110/111/112` | Default foreground/background/cursor RGB set/query and explicit reset. Sets preserve configured alpha; query mirrors BEL/ST termination. |
| OSC `7` | Validated working-directory URI metadata, not chdir/filesystem authority. |
| OSC `8` | Bounded cell-linked hyperlinks. Parsing/rendering never opens or fetches a URI. |
| OSC `52` | Bounded UTF-8 system-clipboard **write request** subset only; no reads, non-system selections or terminal-owned OS clipboard access. |
| OSC `133` | Prompt/command markers A/B/C/D and optional command-finished exit code; metadata, not a command executor. |
| Other DCS/APC/PM/SOS | Framing/discard paths do not implement graphics, file transfer or arbitrary passthrough. |

Owners: [OSC dispatch/parsing](../../crates/harbor-terminal/src/parser/osc.rs), [OSC action tests](../../crates/harbor-terminal/src/parser/osc_action_tests.rs), [OSC 52 contract tests](../../crates/harbor-terminal/tests/osc52_contract.rs). Full details: [OSC checklist](../protocol/checklist.md#24-osc-basic-parsing), [DCS](../protocol/checklist.md#25-dcs-basic-parsing), [APC/PM/SOS](../protocol/checklist.md#26-apc-pm-and-sos).

## Safe examples

These byte recipes use controlled text and can be fed to `Terminal::put_bytes` in the [headless program](README.md#minimal-headless-use). They are examples, not transcripts of new tests. Avoid feeding experiments to a production interactive session; cursor/erase/alternate-buffer commands mutate its display.

```rust
use harbor_terminal::Terminal;

fn main() {
    let mut terminal = Terminal::new_headless(3, 20);
    terminal.put_bytes(b"\x1b[31mred\x1b[0m");
    assert_eq!(terminal.screen().cell(0, 0).raw_text(), "r");

    // CUP row 2, column 3, then ordinary source text.
    terminal.put_bytes(b"\x1b[2;3Hok");
    assert_eq!(terminal.snapshot().cell_char(1, 2), 'o');

    // A styled underline with a separate RGB underline color.
    terminal.put_bytes(b"\x1b[3;1H\x1b[4:3;58:2::255:80:80mcurve\x1b[0m");
    assert_eq!(terminal.snapshot().cell_char(2, 0), 'c');
}
```

Wire-only query example (requires an actual reply-reading transport, not the direct headless feed above):

```text
send:    ESC [ 6 n
receive: ESC [ row ; col R

send:    ESC P + q 544E ESC \        # hex "TN"
receive: ESC P 1 + r 544E=787465726D2D323536636F6C6F72 ESC \
```

## Deliberate exclusions and proof gaps

Kitty keyboard/graphics, Sixel rendering, iTerm-style OSC 1337 file/image transfer, OSC 52 reads, tmux DCS passthrough wrappers, modifyOtherKeys and broad window-manipulation controls are not supported contracts here. Well-framed payload discard is not a positive capability reply. Complex script shaping and font ligatures remain outside the current text contract. Do not advertise a Kitty/Ghostty identity because one modern extension works.

Use the checklist's [keyboard](../protocol/checklist.md#31-keyboard-mode-related-protocols), [window](../protocol/checklist.md#23-window-operations) and [final acceptance](../protocol/checklist.md#39-final-acceptance) sections for granular gaps. Runtime records linked elsewhere retain their original application/OS/transport exclusions; this reference adds no new passes and no native Unix claim.

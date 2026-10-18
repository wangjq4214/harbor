# VT reference: implemented behavior and deliberate limits

[Terminal core](README.md) · [Compatibility](compatibility.md) · [Security](security.md)

This reference describes common supported escape sequences, replies and limits. Application behavior also depends on the host, renderer and transport; see [compatibility](compatibility.md).

## Notation and framing

- `ESC` = byte `0x1b`; `CSI` = `ESC [`; `OSC` = `ESC ]`; `DCS` = `ESC P`; `ST` = `ESC \`.
- Examples below use 7-bit introducers and UTF-8 text. The parser has opt-in 8-bit C1 handling, but `Terminal` does **not** enable it by default. UTF-8 text is not permission to reinterpret continuation bytes as C1 controls.
- `Ps` is a numeric parameter; `Pn` is usually a count. Missing, explicit zero and colon subparameters are sequence-specific, not interchangeable across all commands.
- OSC accepts BEL or ST termination. DCS/APC/PM/SOS use string framing; CAN/SUB cancel unfinished work. Incremental input may split anywhere, including within UTF-8, ESC or ST.
- Bounded consumption of an unknown string family is **safe discard**, not implementation of the feature carried inside it. See [resource limits](security.md#resource-limits-and-what-they-do-not-bound).

## Text, cursor and editing

The following control and editing families are supported:

| Family | Common wire forms | Contract / detail |
| --- | --- | --- |
| Basic controls | BS, HT, LF/VT/FF, CR, SO/SI | Backspace, tab, line feed, carriage return, charset invocation. BEL/ENQ are consumed without a bell/answerback host effect. |
| Cursor movement | `CSI Pn A/B/C/D`, `E/F`, `G`, `H/f`, `d`; aliases `a/e` and HPA | Rows/columns are 1-based on the wire; active margins/origin constrain movement. |
| Save/restore | `ESC 7/8`, `CSI s/u`, `?1048 h/l` | `CSI s` instead sets left/right margins when margin mode is enabled. |
| Erase | `CSI Ps J/K`, `CSI Pn X`, selective `CSI ? Ps J/K` | Erase uses current blank/protection policy; display erase mode 3 handles scrollback. |
| Character edits | `CSI Pn @/P/b` | Insert, delete, repeat. Wide text units require whole-unit invariants. |
| Lines/scroll | `CSI Pn L/M/S/T`, `ESC D/E/M` | Insert/delete lines, scroll regions, IND/NEL/RI. |
| Margins/tabs | `CSI top;bottom r`, `CSI left;right s`, `ESC H`, `CSI Ps g`, `CSI Pn I/Z` | Scroll region, left/right margins, set/clear tab stops and forward/back tab. |
| Charset/alignment | `ESC (/)/*/+` designation, `ESC N/O`, `ESC # 8` | Supported designations include DEC graphics; not every ISO-2022 charset. DECALN fills the visible grid with `E`. |
| Rectangles/protection | `CSI ... $ z/{/x/v/r/t`, `CSI Ps " q` | DECERA/DECSERA/DECFRA/DECCRA/DECCARA/DECRARA; DECSCA selective-erase protection. |
| Reset | `ESC c`, `CSI ! p` | RIS and DECSTR differ. Do not assume a soft reset clears session metadata, palette or nested synchronized-output state. |

## Rendition and modes

SGR supports basic/bright/256/RGB colors, intensity, italic, inverse, conceal, strike, overline and modern underline style/color. Typical forms include `31`, `38;5;n`, `38;2;r;g;b`, `4:0..5`, `58:2::r:g:b`, `24`, `59` and `0` followed by `m`. Colon/semicolon color forms have their own validation. Storing an attribute such as blink does not establish a visible text-blink animation contract. Conceal does not remove source text.

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

Modes are set/reset by `h/l`; unsupported mode queries report unknown. `?9` press-only X10 tracking is not implemented; legacy `CSI M` **encoding** under supported tracking is a different capability. Legacy coordinates above 223 and UTF-8 coordinates above 2015 are suppressed, not wrapped.

## Queries and replies

| Request | Current reply / restriction |
| --- | --- |
| `CSI 5 n` | `CSI 0 n` (status OK). |
| `CSI 6 n`, `CSI ? 6 n` | Standard/private CPR with current 1-based cursor coordinates respecting the implemented origin/margin rules. |
| `CSI c` or `CSI 0 c`, `ESC Z` | Primary DA / DECID: `ESC [ ? 62 ; 6 ; 17 ; 22 ; 28 c`. |
| `CSI > c` or `CSI > 0 c` | Secondary DA: `ESC [ > 1 ; 1 ; 0 c`. Fixed protocol identity, not the application build version. |
| `CSI Ps $ p`, `CSI ? Ps $ p` | DECRQM/DECRPM mode status; only the supported request shape. |
| `DCS $ q Pt ST` | DECRQSS for implemented SGR, scroll/margin, cursor-style and protection status strings; not arbitrary terminal state. |
| `DCS + q hex-names ST` | XTGETTCAP registry: `TN=xterm-256color`, `RGB=8/8/8`, numeric `Co/colors=256`, numeric `U8=1`, and lowercase `u8` and `Su` name-only booleans. Unknown/malformed names are skipped; no match/overflow produces the defined failure frame. |

Tertiary DA (`CSI = c`), XTVERSION (`CSI > q` / `CSI > 0 q`), and the Kitty keyboard query (`CSI ? u`) intentionally produce no reply; no Kitty identity/version is claimed. Numeric XTGETTCAP values are decimal text before hex encoding (`Co=256` → `436F=323536`, `U8=1` → `5538=31`), not name-only booleans. Uppercase `U8` controls Unicode-versus-ACS line drawing in UTF-8 locales; legacy lowercase `u8` remains distinct and has a known ncurses naming collision. Multiple-name handling differs from xterm's stop-at-first-unknown behavior. This is not a complete terminfo database.

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

## Safe examples

These byte recipes use controlled text and can be fed to `Terminal::put_bytes` in the [headless program](README.md#minimal-headless-use). Avoid feeding experiments to a production interactive session; cursor/erase/alternate-buffer commands mutate its display.

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

## Unsupported features

Kitty keyboard/graphics, Sixel rendering, iTerm-style OSC 1337 file/image transfer, OSC 52 reads, tmux DCS passthrough wrappers, modifyOtherKeys and broad window-manipulation controls are not supported contracts here. Well-framed payload discard is not a positive capability reply. Complex script shaping and font ligatures remain outside the current text contract. Do not advertise a Kitty/Ghostty identity because one modern extension works.

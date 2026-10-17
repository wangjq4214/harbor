# OSC 4/104 Indexed Palette

**Spec ID:** 0019
**Status:** Implemented ([scoped source, tests, GPU and Windows probe evidence](../../docs/osc-palette-acceptance.md))
**Date:** 2026-10-10
**Sources:** [Issue #191](https://github.com/wangjq4214/harbor/issues/191), the user's approval of the recommended contract and Spec-only endpoint on 2026-10-10, [ADR 0053](../adr/0053-bounded-session-osc-palette-policy.md), [Protocol Checklist](../../docs/protocol/checklist.md#243-palette), and [Validation Policy](../../docs/validation.md).

## Scope and Settled Contract

Deliver bounded OSC 4 palette set/query and OSC 104 palette reset so an application's queries and displayed indexed colors agree. This is the indexed-color slice of N11, separate from the existing OSC 10/11/12/110/111/112 default-color handlers.

The approved compatibility choices are indexes 0-255 only, the existing hexadecimal RGB subset, whole-request validation before effects, ordered execution of valid operations, alpha preservation on set, and startup RGBA restoration on explicit palette reset. Dynamic indexed colors are session-owned, shared across primary/alternate buffers, and preserved by RIS, DECSTR, and SGR reset. No blocking semantic decision remains for this contract.


## Requirements

### R1. Complete bounded indexed palette

- Support each indexed entry 0-255 as an independently settable, queryable, and resettable color. Reject invalid indexes, including negative values, 256 and larger values, and numeric overflow; do not clamp into another slot or grow palette storage.
- Startup entries 0-7 and 8-15 use the terminal session's configured normal and bright ANSI colors. Entries 16-231 and 232-255 initially retain Harbor's existing color-cube and grayscale colors. Explicit reset returns to this startup baseline, not an unrelated compiled ANSI palette or another session's configuration.
- `Color::Named(n)`, `Color::Bright(n)`, and the corresponding `Color::Indexed` entries resolve consistently for their valid ANSI indexes. A runtime mutation must not update only one of these paths.
- Maintain one authoritative active palette for queries and presentation. The concrete storage layout and private type names are not prescribed.

### R2. OSC 4 validation and ordering

- Accept one or more `index;color` pairs: `OSC 4 ; index ; color [ ; index ; color ... ] BEL/ST`. A color is either a supported RGB value or the exact standalone `?` query token.
- Accept `#RRGGBB` and `rgb:R/G/B`, with one to four hexadecimal digits per RGB component, including upper/lowercase hexadecimal digits. Preserve the existing default-color component scaling and rounding behavior: normalize each component to an 8-bit channel; do not introduce a different precision policy for indexed colors.
- Reject unsupported color names, `rgbi:`, alpha-bearing forms, illegal hexadecimal digits, missing/extra components, malformed pair structure, and invalid indexes. Embedded or appended question marks are not query tokens.
- Validate every pair and the full payload shape before changing any entry or queuing any reply. If any pair is invalid, consume-ignore the entire request, including otherwise-valid leading sets or queries. Unknown payload shapes have no reply or palette side effect.
- Execute valid set/query pairs in appearance order. Queries observe preceding sets in the same request, but not later sets. Repeated indexes are allowed and retain this ordering.
- A set replaces only RGB and preserves the active entry's alpha. Sets produce no query reply.

### R3. Exact, bounded query replies

- For a valid `OSC 4 ; index ; ?`, return `ESC ] 4 ; index ; rgb:rrrr/gggg/bbbb` followed by the request's BEL or ST terminator. The output index denotes the queried entry; RGB hexadecimal output is lowercase.
- Use the same 8-bit quantization and repeated-byte 16-bit channel representation as OSC 10/11/12 queries; queries report the active rendered RGB and do not serialize alpha.
- For example, setting entry 42 to `#123456` and querying it with ST yields exactly the following bytes (Rust byte-string notation):

```rust
b"\x1b]4;42;rgb:1212/3434/5656\x1b\\"
```

- BEL termination yields the same body ending in `\x07`, without ST. Multiple query pairs produce separate replies in request order, subject to the existing reply-buffer bound.
- Use the existing platform-neutral TerminalReply path, with its 1024-byte cumulative buffer limit and atomic acceptance/drop of each complete reply. Do not write directly to a platform PTY from the parser or bypass capacity checks.
- A three-digit index with ST requires 28 bytes. Reply construction must safely accommodate the longest supported reply; the existing default-color formatter's 26-byte capacity is not sufficient by itself.
- Whole-request validation does not reserve capacity for all replies as one transaction. Preserve the existing complete-reply drop behavior at capacity; never leave a truncated reply. Reply capacity does not change the ordered semantics of valid palette sets.

### R4. OSC 104 reset and isolation

- `OSC 104 BEL/ST` with an empty payload restores all 256 indexed entries to their startup RGBA values.
- `OSC 104 ; index [ ; index ... ] BEL/ST` restores only the specified indexed entries. Repeated valid indexes are supported. Validate the full nonempty index list before any restoration; an invalid entry rejects the whole request.
- Reset produces no reply. Unsupported reset payload shapes are consumed without effects.
- Full or selective indexed reset must leave the active default foreground/background, cursor, and selection colors unchanged. OSC 110/111/112 continue to reset only their existing default-color slots.
- The session's active indexed palette survives primary/alternate screen transitions, including the existing parked-alternate behavior, and RIS, DECSTR, and SGR reset. Those operations retain all unrelated existing semantics. A second terminal session's colors must remain unaffected by the first session's sets or resets.

### R5. Coherent rendering and retained updates

- Already-written semantic indexed cells and subsequent output resolve through the updated palette. Changes affect applicable foreground, background, and decoration colors, including ANSI aliases. Truecolor values are not altered by indexed palette mutation.
- Preserve cell text, semantic color indexes, widths, and unrelated attributes; do not replace stored indexed colors with concrete RGB at write time to implement this feature.
- A changed palette must invalidate relevant retained render colors and make the changed display available through the existing damage/update contract. Hidden/skipped frames must not lose the palette change; the next rendered update uses current colors.
- Palette changes between reading and acknowledging an engine update must not allow an obsolete projection to consume pending changes. Keep the existing engine/renderer ownership and coherent-update guarantees of [ADR 0045](../adr/0045-gpu-independent-terminal-core-boundary.md).
- Parser/model/reply behavior remains GPU-independent; actual rendering evidence is a separate acceptance obligation.

### R6. Framing, limits, and existing compatibility

- Use the established OSC framing and static typed-action routing. OSC 4/104 support BEL and ST completion and are equivalent under one-shot and fragmented ingestion. Incomplete sequences have no completed palette effects.
- Preserve the existing bounded OSC retention contract (4096 bytes for the ordinary OSC path), cancellation, overflow consume-ignore behavior, and recovery to following text or valid commands. Any handler-owned action collection or allocation must be bounded by the accepted payload; a fixed palette size alone does not prove temporary allocations are bounded.
- Do not expand unrelated parser retention limits to support this slice. Preserve OSC 52's separate capacity/policy and other OSC routing.
- Preserve the exact existing OSC 10/11/12/110/111/112 set/query/reset forms, alpha behavior, replies, invalid-input handling, and lifetimes under [ADR 0036](../adr/0036-preserve-harbor-alpha-for-osc-default-colors.md). Sharing RGB helpers must not change those commands' single-color payload semantics.

### R7. Delivered evidence and honest documentation

- Provide focused parser/terminal tests for single/multiple sets, mixed set/query ordering, exact bytes, full/selective/repeated-index reset, malformed colors, bounds, fragmented input, and BEL/ST terminators, as required by #191.
- Update checklist sections 24.3 and the OSC 104 items in 24.4 only after implementation and named supporting tests exist. Do not interpret #191's shorthand about unchecked items as meaning the already-implemented default-color items in 24.4 are unchecked.
- Record Windows runtime evidence for a named, versioned application workload or palette set/query/reset probe through the actual Harbor/ConPTY session. A synthetic parser test, successful PTY write, or GPU-independent model result is not Windows application acceptance.
- Evidence summaries follow [Validation Policy](../../docs/validation.md): revision/dirty-tree scope, environment and Windows/ConPTY/application versions, reproducible commands/steps, expected/observed results, PASS/FAIL/NOT RUN/BLOCKED, and known exclusions. Keep raw artifacts local or link retrievable CI artifacts; do not commit local-only artifact links or unrelated private terminal contents.

## Verification

The [acceptance record](../../docs/osc-palette-acceptance.md) maps R1–R7 to focused tests, retained GPU readback and the versioned Windows palette probe, with explicit exclusions. Apply the [standard quality gates](../../docs/validation.md#standard-quality-gates); source or skipped GPU/native checks alone are not runtime acceptance.

## Out of Scope

- Negative/special indexes such as -1, indexes above 255, color names, `rgbi:`, alpha-bearing color syntax, and complete historical xterm color syntax.
- Changing existing default-color commands, selection colors, OSC 52 policy/capacity, other OSC payload semantics, or reply-buffer capacity/drop policy.
- Configuration hot reload, new TOML color syntax, live rebasing of the startup reset baseline, or cross-session palette sharing.
- Changing terminal identity/advertised unrelated capabilities, claiming unexercised transports, or delivering other N11/Kitty keyboard/graphics work.

# OSC 4/104 Indexed Palette

**Spec ID:** 0019
**Status:** Implemented ([scoped source, tests, GPU and Windows probe evidence](../../docs/osc-palette-acceptance.md))
**Date:** 2026-10-10
**Sources:** [Issue #191](https://github.com/wangjq4214/harbor/issues/191), the user's approval of the recommended contract and Spec-only endpoint on 2026-10-10, [ADR 0053](../adr/0053-bounded-session-osc-palette-policy.md), [Protocol Checklist](../../docs/protocol/checklist.md#243-palette), and [Validation Policy](../../docs/validation.md).

## Scope and Settled Contract

Deliver bounded OSC 4 palette set/query and OSC 104 palette reset so an application's queries and displayed indexed colors agree. This is the indexed-color slice of N11, separate from the existing OSC 10/11/12/110/111/112 default-color handlers.

The approved compatibility choices are indexes 0-255 only, the existing hexadecimal RGB subset, whole-request validation before effects, ordered execution of valid operations, alpha preservation on set, and startup RGBA restoration on explicit palette reset. Dynamic indexed colors are session-owned, shared across primary/alternate buffers, and preserved by RIS, DECSTR, and SGR reset. No blocking semantic decision remains for this contract.

This authoring task creates a spec and records qualifying durable policy. It does not authorize production changes, a separate implementation-plan stage, ticket decomposition, GitHub edits, staging, or committing.

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

## Solution and Necessary Seams

Extend the existing terminal-owned indexed color resolution and OSC action path. The active indexed palette and startup reset baseline must participate in the coherent engine update already consumed by the renderer. No new parser, direct PTY reply owner, universal renderer abstraction, or configuration format is selected.

| Seam | Connects | Expects | Provides |
| --- | --- | --- | --- |
| Startup configuration -> session palette | `harbor-config` palette -> terminal session color state | Configured normal/bright RGBA plus existing cube/grayscale baseline | Independent startup reset baseline and bounded active indexed colors |
| Completed OSC -> typed actions | `harbor-parser` framing -> terminal OSC routing/handler | Completed bounded command/payload and BEL/ST termination fact | Full-request validation, ordered set/query/reset semantics, consume-ignore for malformed candidates |
| Semantic colors -> effective colors | Cell/decoration indexed values and ANSI aliases -> active palette resolution | Current session palette, existing semantic cell data | Matching indexed/ANSI colors without rewriting retained text or truecolor |
| Active palette -> renderer update | Session color state -> `TerminalUpdate` -> render pipeline/layers | Coherent palette and damage, acknowledgement only for current projection | Updated retained foreground/background/decoration colors, replay after hidden/skipped frames |
| Query -> application | OSC query handler -> Screen TerminalReply -> existing PTY delivery | Active RGB, request terminator, bounded complete replies | Exact ordered protocol bytes through the current platform-neutral path |
| Buffer/session lifecycle -> palette lifetime | Existing primary/alternate swap and reset operations -> session color state | Session ownership distinct from buffer contents and pen reset | Palette survives approved transitions and stays isolated across sessions |

### Inspected repository anchors

- `crates/harbor-config/src/color.rs`: current `Palette`, semantic `Color`, and computed indexed-color resolution. The current type stores ANSI normal/bright entries, not mutable slots for 16-255.
- `crates/harbor-terminal/src/screen/default_colors.rs` and `screen.rs`: startup/active default colors, changed-color damage, alternate-screen state transfer, and the 1024-byte reply buffer.
- `crates/harbor-terminal/src/parser/osc.rs`, `osc_color.rs`, and `handlers.rs`: typed OSC routing, supported RGB parsing/rounding, default-color reply formatting, and action application.
- `crates/harbor-terminal/src/lib.rs` and `update.rs`: active palette projection and rejection of stale update acknowledgements.
- `crates/harbor-terminal/src/render/pipeline.rs`: palette synchronization into retained render layers.
- `crates/harbor-terminal/src/parser/tests.rs`, `parser/incremental_tests.rs`, `screen/tests.rs`, `core_tests.rs`, `terminal_tests.rs`, and render tests: existing regression and integration surfaces.

These anchors describe the inspected integration points, not delivered OSC 4/104 implementation. Choosing private storage and helper organization remains execution detail within this contract.

## End-to-End Tests

All byte strings below use Rust escape notation; tests must compare protocol bytes, not the printed escape spelling.

| Case | Given / input | Observable outcome |
| --- | --- | --- |
| Existing content recolored | Write indexed foreground/background/decoration content at slots spanning 0, 15, 16, 231, 232, 255; then set those entries through OSC 4 | Previously written and newly written content use the changed colors; ANSI aliases agree; retained text/width/attributes and truecolor comparison content are unchanged |
| Exact ST query | Set 42 to `#123456`, then feed `b"\x1b]4;42;?\x1b\\"` | Exact `b"\x1b]4;42;rgb:1212/3434/5656\x1b\\"` reply and matching rendered RGB |
| Short/mixed-width RGB and BEL | Feed `b"\x1b]4;255;rgb:f/80/8000;255;?\x07"` | Entry 255 uses RGB `(255,128,128)`; exact `b"\x1b]4;255;rgb:ffff/8080/8080\x07"` reply; alpha is preserved |
| Multi-entry ordered mutation | Feed `b"\x1b]4;42;#112233;43;#abcdef;42;?;42;#445566;42;?\x1b\\"` | Slots 42/43 change; replies report `1111/2222/3333` then `4444/5555/6666`; final slot 42 is `#445566` |
| Invalid tail rejects leading effects | Feed `b"\x1b]4;42;#112233;42;?;256;#abcdef\x1b\\"` or a payload with a valid prefix and incomplete trailing pair | No palette mutation or reply, including the leading set/query; following ordinary text and valid OSC still work |
| Invalid color/query shapes | Valid pair prefix followed by `red`, `red?`, `rgbi:1/0/0`, `#12345678`, invalid hex, missing components, or unsupported component width | Entire request is consumed without mutation or reply; exact standalone `?` remains the only query token |
| Selective reset with repeats | Configured ANSI entries plus changed slots 1, 42, 255; feed `b"\x1b]104;1;42;42\x1b\\"` | Entries 1/42 regain startup RGBA; 255 stays changed; no reply; defaults/cursor/selection stay unchanged |
| Invalid reset list | Changed slots 1/42; feed `b"\x1b]104;1;256\x07"` or a malformed index list | Neither slot is restored; no reply; later valid reset succeeds |
| Full indexed reset separation | Mutate indexed slots and OSC default colors, then feed `b"\x1b]104\x07"` | All indexed slots return to startup ANSI/cube/grayscale colors; OSC default colors, cursor, and selection remain as before this reset |
| Buffer and reset lifetime | Set/query in primary and alternate buffers using existing mode families, return/re-enter parked alternate, then RIS/DECSTR/SGR reset and explicit OSC 104 | One active session palette survives all approved transitions; explicit OSC 104 restores it; other reset semantics remain unchanged |
| Session isolation | Two terminals with distinct startup ANSI configurations; mutate/reset one session | The other session's query/display values are unchanged; each reset uses its own baseline |
| Fragmentation and framing recovery | Feed equivalent streams whole and at splits within indexes/colors, between ESC and ST final byte, with BEL/ST, cancellation and over-limit requests | Equivalent valid completed outcomes; no incomplete/cancelled/overflow side effects; following text/commands recover |
| Reply capacity | Query three-digit slots with exact remaining capacity and one byte less; issue a bounded multi-query request | Whole replies accepted/dropped without fragments, panic, or cumulative buffer growth above 1024 bytes; emitted replies preserve order |
| Retained update and visibility | Read an update, mutate palette, attempt old acknowledgement; skip rendering while colors change, then resume | Obsolete update cannot consume new colors/damage; next coherent projection redraws current indexed colors |
| Existing default colors | Repeat existing OSC 10/11/12/110/111/112 tests alongside OSC 4/104 | Existing exact bytes, supported/invalid forms, alpha and reset/lifetime behavior stay unchanged |
| Windows application path | Run a named/versioned set/query/reset workload in actual Harbor through ConPTY, with visible indexed samples and captured replies | Application-visible colors and exact replies agree; selective/full reset works; observations, environment and transport exclusions are recorded |

## Decisions and Traceability

| Assertion / choice | Authority |
| --- | --- |
| Full indexed set/query/reset outcome, bounded actions, invalid input, exact replies and required gates | Issue #191 |
| 0-255 subset, RGB formats, alpha, whole-request validation, ordered mixed operations, explicit reset lifetime | User approval of the recommendation table; ADR 0053 |
| Existing default-color contract is unchanged | Issue #191; ADR 0036; inspected `osc_color.rs` |
| Startup ANSI colors and existing cube/grayscale baseline; consistent ANSI aliases | Approved implementation boundaries; inspected `harbor-config/src/color.rs` |
| RGB scaling, lowercase repeated-byte queries, 28-byte longest palette reply | Existing `parse_component`/`format_query` semantics applied to approved OSC 4 syntax; inspected source and byte-length derivation |
| Complete-reply capacity/drop and existing OSC retention bound | ADR 0017; TerminalReply and Parser Retention Limits context; inspected `Screen::push_reply` |
| Coherent update, pending damage and GPU-independent ownership | Approved rendering boundaries; ADR 0045; inspected `read_update`, `acknowledge_update`, and renderer synchronization |
| Named Windows workload, truthful checklist/evidence claims | Issue #191; Validation Policy |

No additional domain fact, architecture, precision policy, special-slot support, or unconfirmed assumption is introduced. A material new protocol, lifetime, ownership, or acceptance choice discovered during execution must return to clarification and durable recording before this contract is changed.

## Verification and Definition of Done

- [ ] R1-R4 have focused deterministic tests for full range/baseline, set/query ordering, exact bytes, resets, alpha, malformed requests and session isolation.
- [ ] R5 has semantic model/update tests and retained renderer evidence for recoloring foreground/background/decoration, ANSI aliases, unchanged truecolor, hidden-frame replay and stale acknowledgements.
- [ ] R6 has one-shot/fragmented, BEL/ST, cancellation, over-limit/recovery and bounded temporary-action/reply coverage; existing default colors and OSC routes pass applicable regressions.
- [ ] R7 has named Windows/ConPTY runtime evidence and self-contained summaries with honest exclusions.
- [ ] Checklist 24.3 and OSC 104 coverage in 24.4 cite implementation and named tests; no unsupported broader color/transport claim is made.
- [ ] Required quality checks are recorded with revision, environment, results and reasons for any command not run.

Run at implementation boundaries and before merge:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --workspace
python scripts/check_docs.py
python scripts/checklist_summary.py
```

A docs-only spec check is not an implementation pass. A GPU test skipped for lack of an adapter, or an unexecuted Windows procedure, is not runtime acceptance. This draft contains no new test-execution or application-acceptance claim.

## Out of Scope

- Negative/special indexes such as -1, indexes above 255, color names, `rgbi:`, alpha-bearing color syntax, and complete historical xterm color syntax.
- Changing existing default-color commands, selection colors, OSC 52 policy/capacity, other OSC payload semantics, or reply-buffer capacity/drop policy.
- Configuration hot reload, new TOML color syntax, live rebasing of the startup reset baseline, or cross-session palette sharing.
- Changing terminal identity/advertised unrelated capabilities, claiming unexercised transports, or delivering other N11/Kitty keyboard/graphics work.
- New parser/PTY ownership, GPU-dependent core state, implementation plans/tickets, production changes or GitHub mutations during this Spec-only task.

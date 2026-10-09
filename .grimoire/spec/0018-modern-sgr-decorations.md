# Modern SGR Decorations

**Spec ID:** 0018
**Status:** Draft
**Date:** 2026-10-09
**Sources:** [Issue #162](https://github.com/wangjq4214/harbor/issues/162), the user-approved modern SGR refinement contract below, and [ADR 0051](../adr/0051-modern-sgr-decoration-and-blank-retention-policy.md).

## Scope and Settled Contract

Deliver the modern SGR slice of #162, with Neovim as the first named application workload: styled and colored underlines, conceal/reveal, and overline. The user selected Spec -> slice and approved all recommended compatibility rules after reviewing the impact on primary reflow. This spec does not authorize implementation by itself.

For sequencing, the user directs that all existing #162 sub-issues, currently #173 and #174, are treated as fully implemented and verified. Do not reopen their implementation or historical acceptance as prerequisites. This is the supplied prerequisite premise, not a new execution result or permission to rewrite historical evidence.

The remaining OSC 4/104, mouse, box-drawing, and broader SSH/tmux capability work stays in #162 outside this delivery. Necessary SGR state/status/discovery integration belongs here; it is not a separate terminal-identity project.

## Requirements

### R1. Underline style and compatibility

Represent one authoritative explicit underline style in pen state and written cells. It must be distinguishable from OSC 8's renderer fallback, and must not be reduced to an on/off flag in snapshots or retained logical content.

| SGR input | Explicit underline result |
| --- | --- |
| `4`, `4:1` | Single |
| `21`, `4:2` | Double; `21` does not clear bold |
| `4:3` | Curly/wavy |
| `4:4` | Dotted |
| `4:5` | Dashed |
| `24`, `4:0` | Off |

- Unknown underline styles preserve the prior explicit style; malformed/unsupported candidates must be consumed safely without corrupting subsequent text or unrelated attributes.
- Honor top-level SGR ordering, including resets and mixed semicolon/colon parameters. Colon subparameters must not be reinterpreted as independent top-level attributes.
- SGR changes affect subsequent writing, not retroactively restyle already written cells.
- Preserve existing bold, dim, italic, blink, inverse, strikethrough, and foreground/background behavior outside the selected extensions.

### R2. Independent underline color

Support the selected indexed/RGB SGR 58 family alongside existing foreground/background colors:

| Form | Meaning |
| --- | --- |
| `58;5;n`, `58:5:n` | Indexed underline color |
| `58;2;r;g;b`, `58:2:r:g:b` | RGB underline color |
| `58:2::r:g:b` | RGB with the empty colorspace field |
| `59` | Restore following the effective foreground |

- Index and RGB components are in `0..255`. Out-of-range and incomplete candidates leave the previous underline color unchanged rather than clamp, truncate, or reset it.
- Underline color is independent of underline enablement: changing color does not enable a style, and `24`/`4:0` disable the explicit style without clearing its color. Re-enabling a style uses the retained color; `59` changes color without disabling style.
- Default underline color follows the effective rendered foreground, including inverse. An explicitly assigned underline color remains that color under inverse rather than swapping with foreground/background.
- Keep indexed color semantic until palette resolution, and use the terminal's existing active palette path. This slice does not add OSC 4 palette mutation or a second palette owner.
- Preserve existing 38/48 parsing and rendering; shared color helpers are implementation details, not authorization to broaden their protocol semantics.

### R3. Decorations, spaces, and hyperlink interaction

- Render all enabled explicit underline styles on glyph-bearing cells and spaces. Double, curly, dotted, and dashed must be visibly distinguishable from single underline.
- SGR `53` enables overline; `55` disables it independently of underline and strikethrough. Overline uses the effective text foreground and also paints spaces.
- Adjacent cells of a continuous same-style decoration must not acquire cell-boundary gaps from independent per-cell placement. Wide lead/continuation cells together cover the text unit's occupied width without a missing half or duplicate overlapping decoration.
- Decoration geometry must remain correctly positioned and clipped under existing font/size, viewport, and DPI transitions. Retained GPU projections must not reuse stale decoration geometry or color after relevant invalidation.
- Preserve existing non-space OSC 8 automatic single-underlining. An enabled explicit SGR underline style takes precedence; when that style is off, the existing hyperlink fallback still applies. Hyperlinked spaces do not gain an automatic underline merely because of OSC 8.
- SGR reset does not close a hyperlink. Preserve OSC 8 open/close/reset and activation behavior under [ADR 0035](../adr/0035-cell-linked-bounded-registry-for-osc8-hyperlinks.md).
- This requirement does not expand existing strikethrough-on-space behavior; strikethrough must continue to coexist correctly with the selected new attributes.

### R4. Conceal is presentation, not redaction

- SGR `8` enables conceal; `28` disables it for subsequent writing. Existing written cells keep their stored attributes.
- Concealed cells produce no foreground glyph, underline, automatic hyperlink underline, strikethrough, or overline. Conceal still applies when inverse or an explicit underline color is active.
- Preserve cell background, source text (including suffix scalars), assigned grid width, cursor advancement, retained content, selection meaning, and copied original text. Conceal does not replace text with spaces, delete it, or turn it into disposable blank capacity.
- Conceal is not a security guarantee. Do not describe copied/retained concealed text as redacted or secret-protected.
- Normal cursor, selection, IME, and host input behavior must retain their existing contracts; this slice does not redesign them.

### R5. State lifetime, meaningful blanks, and resize

- Carry explicit underline style/color, overline, and conceal through pen state, cell construction, wide continuation construction, saved pen state, edits/movement, retained history, logical content, and renderer snapshots.
- Include the new state in existing DECSC/DECRC and CSI cursor-save/restore pen semantics. Keep primary/alternate buffer isolation and existing alternate-screen mode-family behavior.
- SGR `0`/omitted SGR and DECSTR restore the new pen attributes to their defaults, alongside their existing behavior. RIS restores defaults and existing buffer/reset behavior. Defaults are no explicit underline, foreground-following underline color, no overline, and no conceal. Attribute-specific resets change only their selected state.
- Visibly decorated spaces are meaningful retained content, including where existing styled erase/fill operations create them. Update blank provenance and row meaningful extents consistently with renderer visibility; do not fix only explicit writes.
- Primary narrowing/widening preserves decorated blanks and their new state. Conceal's temporary visibility suppression must not discard retained styled contents.
- Preserve [ADR 0046](../adr/0046-trim-ordinary-trailing-spaces-during-primary-reflow.md): ordinary default-style, unprotected, non-hyperlinked logical tail spaces can still be discarded, subject to live/saved cursor exceptions. Do not turn all unused cells into retained decorated content or change unrelated output/erase semantics.
- Primary reflow and alternate-screen rectangular resize remain distinct. Wide-cell normalization, content anchors, selection projection, history eviction, and transactional resize rules continue under [spec 0014](./0014-content-preserving-resize-reflow.md) and its applicable ADRs.
- Current `retained_glyph_count` already preserves explicit underlined spaces through its non-default-cell check. The new work extends state/provenance consistency; it is not evidence of a pre-existing blanket loss of underlined tails.

### R6. Accurate status replies and discovery

- Existing DECRQSS SGR queries must serialize the actual new pen state, including explicit style/color, overline, and conceal. Replies must be deterministic and reconstruct that state when their returned SGR is applied to a reset pen.
- Preserve existing exact reply bytes for legacy-only/default states where the new state is default. New states need exact-byte tests, including fragmented and cancelled DCS requests, reset, and reply bounds.
- Use the existing DECRQSS framing and bounded TerminalReply path; no parallel PTY reply writer. Keep existing request/reply retention limits and whole-reply behavior.
- Once styled underline implementation and its delivered verification are complete, XTGETTCAP resolves `Su` as the supported boolean capability through the existing registry. Verify the single-capability reply is exactly `ESC P 1 + r 5375 ESC \\`, and that mixed queries, unsupported names, cancellation, and existing output bounds keep their contract.
- Keep `TERM=xterm-256color`, existing DA identities, and existing unrelated capability replies unchanged. Support for colored/styled underlines does not imply Kitty keyboard, graphics, clipboard reads, or Kitty identity.

### R7. Named application and honest evidence

- Demonstrate Neovim undercurl/diagnostic-style highlighting and explicit underline color through an actual Harbor Windows/ConPTY session. Record the Neovim version, launch/transport path, highlight setup, expected versus observed rendering, resize/DPI observations, and return to the shell.
- Supplement Neovim with direct synthetic VT cases for all styles, overline, conceal, explicit spaces, resets, inverse, and status/discovery bytes. A synthetic sequence test alone is not Neovim acceptance.
- Preserve existing reply, OSC metadata, focus, SGR mouse, IME, synchronized output, selection/copy, and alternate-screen behavior through applicable regression checks rather than reimplement them.
- Store retrievable evidence under `docs/verification/` or link CI artifacts using [Validation](../../docs/validation.md). Record revision/dirty scope, Windows/ConPTY/application versions, commands/steps, expected/observed results, PASS/FAIL/NOT RUN/BLOCKED, artifacts, and exclusions. Do not capture secrets or unrelated terminal/clipboard contents.
- Update protocol/status documentation only for behavior actually delivered and evidenced. The approved #162 prerequisites remain outside this slice's revalidation scope.

## Solution and Necessary Seams

Extend the established terminal-owned SGR and cell-state path and the existing renderer projection; do not introduce a new parser, terminal identity, renderer abstraction, or system side-effect owner. Use explicit style semantics and an independent underline color without prescribing private type names, bit widths, shader representation, curve sampling, or tessellation.

| Seam | Connects | Expects | Provides |
| --- | --- | --- | --- |
| Parsed SGR -> authoritative pen | `harbor-parser::Params` -> terminal SGR handler/pen | Preserved top-level parameters and colon subparameters, fixed parser bounds | Ordered validated state changes and safe ignored candidates |
| Pen -> retained cells/saved state | Pen/cell writer/editing/save -> `Cell`, normal buffer and saved pen | Style/color/conceal/overline plus existing text/width/hyperlink state | Complete cell state and save/restore without duplicated authoritative flags |
| Retained content -> resize/copy | Blank provenance/logical decoder -> primary reflow/content projection/copy | Decorated-space significance, existing default-tail and cursor rules | Preserved styled content and anchors; original concealed text remains copyable |
| Snapshot -> GPU projection | Terminal update -> text/decoration/background rendering | Coherent semantic state, active palette, current geometry and damage | Correct visible glyph/decoration suppression and rendering, bounded geometry, invalidation |
| Pen/registry -> replies | Current SGR / terminfo registry -> DECRQSS / XTGETTCAP -> TerminalReply | Accurate state, verified `Su`, existing framing and bounds | Deterministic truthful status/discovery bytes |
| Application -> delivered acceptance | Neovim / synthetic VT probes -> Harbor / Windows ConPTY | Named/versioned real application path and controlled fixtures | Reproducible application, rendering, and regression evidence |

Preserve the GPU-independent engine boundary of [ADR 0045](../adr/0045-gpu-independent-terminal-core-boundary.md). Parser/model tests must remain possible without GPU initialization; GPU and native acceptance remain separate proof obligations.

### Current repository anchors

- `crates/harbor-terminal/src/screen/edit/pen_state.rs`: current `set_sgr`, erase-cell creation, save/restore and resets.
- `crates/harbor-terminal/src/model.rs`: `Cell`, `CellAttrs`, blank significance and snapshots.
- `crates/harbor-terminal/src/screen/edit/cell_writer.rs` and `cell_ops.rs`: written/erased cells, wide units and editing.
- `crates/harbor-terminal/src/normal_buf.rs`: `CellState`, meaningful extents and retained provenance.
- `crates/harbor-terminal/src/logical_content.rs`, `primary_reflow.rs` and `content_anchor.rs`: retained logical payload, default-tail trimming and projection.
- `crates/harbor-terminal/src/render/decoration.rs`, `text.rs`, `background.rs`, `pipeline.rs` and `layout.rs`: effective color, decoration geometry, glyph suppression, palette/viewport invalidation and placement.
- `crates/harbor-text/src/metrics.rs`: current font-derived underline/strikethrough measurements.
- `crates/harbor-terminal/src/parser/status_strings.rs` and `xtgettcap.rs`: current state serialization and compile-time capability registry.
- `crates/harbor-terminal/src/parser/tests.rs`, `incremental_tests.rs`, `screen/tests.rs` and renderer tests: regression surfaces.

These are inspected integration anchors, not claims that the new behavior is implemented.

## End-to-End Tests

| Case | Given / input | Observable outcome |
| --- | --- | --- |
| Neovim styled diagnostic | Actual Neovim in Harbor, named version/path, undercurl and underline color configured | Distinct colored undercurl appears; edits/redraw/resize and exit preserve application/shell rendering |
| All underline styles | Synthetic text including spaces under `4:1..5`, legacy `4`/`21`, and `24`/`4:0` | Distinct requested patterns; decorated spaces remain continuous; `21` keeps bold; explicit off leaves ordinary text undecorated unless hyperlink fallback applies |
| Color independence | Indexed and RGB SGR 58 variants, `24`, re-enable, `59`, then inverse | Disable preserves assigned color, re-enable reuses it, default follows effective foreground, explicit color remains unchanged under inverse |
| Invalid and fragmented SGR | Same stream fed whole and at splits inside CSI/colon/RGB parameters; unknown styles and out-of-range/incomplete colors | Equivalent completed state; affected invalid candidates retain previous state; later text/valid SGR recover without unintended attributes |
| Decorated-tail resize | Main-screen `AB` plus four decorated spaces, cursor outside that tail, shrink six columns to three then widen | Complete decorated logical payload survives and paints after each projection; ordinary default-tail comparison still follows ADR 0046 |
| Styled erase/fill blank | Existing erase/fill operation produces spaces with an enabled visible decoration | Effective row extent includes visible decoration; primary reflow does not lose it merely because it was not explicitly printed |
| Concealed contents | Plain/CJK/suffix-bearing cells with conceal plus underline/strike/overline/inverse | No foreground artifacts; background/width/cursor progression unchanged; selected copy returns original fixture text |
| Attribute lifetimes | New state combined with saved pen, edits, scrollback, alternate-screen transitions, SGR reset, DECSTR and RIS | Complete state is carried or reset according to the existing operation's contract; no primary/alternate leakage or stale state |
| Hyperlink fallback | OSC 8 non-space text and spaces with explicit style on/off and SGR reset | Enabled explicit style takes precedence; off restores non-space single fallback; no automatic linked-space underline or implicit link closure |
| Status and discovery | DECRQSS `m`, XTGETTCAP `Su`, reset/legacy queries and malformed/cancelled requests | Exact truthful bounded replies; status round-trip reconstructs the pen; TERM/DA/unrelated replies remain unchanged |
| Geometry/lifecycle | Existing font/size, viewport, DPI, hidden/retained-frame and restore paths | Decorations stay aligned/clipped; no stale GPU projection, lost damage, or duplicate wide-cell decoration |

## Decisions and Traceability

| Requirement | Settled authority |
| --- | --- |
| R1-R4 exact compatibility and presentation rules | User approval of the recommendation table; [ADR 0051](../adr/0051-modern-sgr-decoration-and-blank-retention-policy.md) |
| Style/color forms, inverse interaction, `Su` discovery | Approved choices plus [upstream underline specification](https://github.com/kovidgoyal/kitty/blob/master/docs/underlines.rst) and [protocol checklist](../../docs/protocol/checklist.md#16-sgr-character-attributes) |
| R5 retention/reset boundaries | Approved reflow explanation, ADR 0051, [ADR 0046](../adr/0046-trim-ordinary-trailing-spaces-during-primary-reflow.md), [spec 0014](./0014-content-preserving-resize-reflow.md), existing state-operation semantics |
| Hyperlink lifetime | [ADR 0035](../adr/0035-cell-linked-bounded-registry-for-osc8-hyperlinks.md) and existing renderer fallback; no hyperlink-policy change |
| Core/render ownership | [ADR 0045](../adr/0045-gpu-independent-terminal-core-boundary.md) |
| R6 truthful replies, no Kitty identity, R7 evidence | User approval, #162 and [Validation](../../docs/validation.md) |
| First Neovim slice and completed prerequisites | Explicit conversation selections; historical evidence is not rewritten |

No private representation or additional colorspace protocol is selected by this spec. A material new protocol, state-lifetime, ownership, or acceptance choice discovered during execution must return to clarification and durable recording before revising this contract.

## Verification and Definition of Done

- [ ] R1-R6 have focused deterministic coverage of supported forms, invalid/fragmented/cancelled input, bounds, reset, ordering and relevant state lifetimes.
- [ ] CPU geometry/model tests and actual GPU encode/render evidence cover styles, spaces, wide cells, color/inverse, conceal and overline.
- [ ] Primary reflow and rectangular alternate-screen tests prove new-state retention while ordinary-tail trimming, cursor/selection anchors and existing resize invariants remain intact.
- [ ] Exact DECRQSS/XTGETTCAP replies and reconstructed state are verified; no unsupported identity/capability is advertised.
- [ ] Windows Neovim and direct synthetic cases are recorded with the precise delivered scope, application/transport versions, observations and honest exclusions.
- [ ] Applicable existing reply/OSC/focus/mouse/IME/synchronized-output and copy/selection regressions remain intact; no prerequisite sub-issue is reopened.
- [ ] Documentation reflects implemented and evidenced support only; #162 remains the tracker for remaining non-SGR work.

Run applicable gates at implementation boundaries, recording every unrun command and why:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --workspace
python scripts/check_docs.py
python scripts/checklist_summary.py
```

A GPU test skipped for lack of an adapter, a compiled binary, or an unexecuted native procedure is not a runtime pass. This draft records requirements only and reports no implementation test or application acceptance result.

## Out of Scope

- OSC 4/104 palette operations, new clipboard behavior, legacy/horizontal/pixel mouse extensions, box-drawing geometry, Kitty keyboard/graphics, and complete historical xterm coverage.
- Changing `TERM`, DA identity, SSH/tmux deployment contracts, or claiming a transport not exercised by this slice.
- New rapid-blink behavior, complex shaping/ligatures, strikethrough-on-space expansion, configuration hot reload, or a font-management redesign.
- Conceal-based redaction, changed selection/copy text, a new reflow algorithm, preservation of every ordinary tail space, or a redesign of alternate-screen resize.
- Reimplementation/revalidation of the approved completed #162 sub-issues as prerequisites.
- Production changes, a separate implementation-plan stage, staging/committing, or creating GitHub sub-issues during this artifact-authoring task.

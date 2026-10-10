# Modern SGR Decoration and Blank Retention Policy

**Status:** Completed
**Date:** 2026-10-09
**Superseded by (Windows ConPTY live-primary tail retention only):** [ADR 0052](./0052-conpty-live-primary-producer-geometry-and-styled-tail-clipping.md), approved 2026-10-09. Full styled-blank retention remains the history/non-ConPTY rule; other modern SGR decisions and this historical status are unchanged.

## Context

Issue [#162](https://github.com/wangjq4214/harbor/issues/162) accepts bounded modern SGR extensions rather than complete historical xterm coverage. The user selected modern SGR with Neovim as the first workload and approved the compatibility rules below after discussing their effect on resize reflow.

Harbor currently stores a single underline flag, draws underline/strikethrough only on non-space cells, and recognizes background or inverse styling as visibly meaningful blank content. Primary reflow already preserves explicitly written underlined spaces because `retained_glyph_count` distinguishes them from `Cell::default()`; claiming that all such spaces are currently trimmed would be incorrect. Erase/fill provenance and meaningful row extents must also recognize newly visible decorations.

The choices discussed were the double-underline versus bold-off interpretation of SGR 21, retaining versus replacing state for unknown underline styles, extending explicit decorations onto spaces versus keeping glyph-only rendering, and treating conceal as presentation versus destructive text removal.

## Decision

- Support underline styles `4:0` through `4:5` (off, single, double, curly, dotted, dashed), with `4` as single and `24` as off. Interpret `21` as double underline without clearing bold. Unknown styles preserve the prior underline state.
- Keep underline style and color independent. SGR 58 supports indexed/RGB colors with semicolon and colon forms, including the empty colorspace RGB form identified in the protocol checklist. Out-of-range or incomplete color candidates preserve the prior underline color. SGR 59 restores following the effective foreground; an explicit underline color does not swap under inverse. SGR 24 disables the style without clearing its color.
- Draw explicit SGR underlines and overlines on spaces as well as glyph-bearing cells. Preserve visibly decorated blanks through primary reflow, including their effective content extent when produced by existing styled erase/fill operations. This does not retain every ordinary trailing space: the default-style tail trimming and cursor exceptions of [ADR 0046](./0046-trim-ordinary-trailing-spaces-during-primary-reflow.md) remain unchanged.
- Preserve OSC 8's existing automatic single underline on non-space linked cells; an enabled explicit SGR underline style takes precedence. Disabling the explicit style leaves the hyperlink fallback applicable. SGR changes do not close hyperlinks; retain the hyperlink lifetime of [ADR 0035](./0035-cell-linked-bounded-registry-for-osc8-hyperlinks.md).
- Implement SGR 8/28 conceal/reveal as foreground presentation. Conceal suppresses glyphs, underlines, strikethroughs, and overlines, but preserves backgrounds, source text, cell width, and copied text. Conceal is not a secrecy or redaction boundary and must not make retained styled text disposable merely because it is hidden.
- Extend existing DECRQSS SGR status replies to represent the new authoritative pen state accurately. Advertise styled underline support through XTGETTCAP's `Su` only after the delivered support is verified. Do not change `TERM` or terminal identity to Kitty.

These are accepted behavior and ownership constraints, not prescriptions for private struct layout, bit allocation, shader choice, or curve tessellation. Logical state remains GPU-independent and the renderer projects it under [ADR 0045: GPU-independent terminal core](./0045-gpu-independent-terminal-core-boundary.md).

## Consequences

- New SGR state must survive cell writing, wide-cell handling, editing, save/restore, buffer transitions, and reflow; existing reset families must include the new pen state.
- Renderer visibility and retained blank-content classification must agree. A row containing only visible explicit decoration may occupy physical rows after narrowing; these are not phantom undecorated rows.
- Tests must distinguish explicitly written decorated spaces, styled erase/fill blanks, ordinary tails, hyperlink fallback, concealed retained contents, and primary versus rectangular alternate-screen resize.
- Underline color resolution, inverse interaction, reset independence, exact replies, and Neovim rendering need their own focused and Windows runtime evidence. This proposed record does not claim implementation or a new execution pass.

## Sources

- User approval in the #162 refinement conversation: all recommended SGR rules accepted; existing sub-issues treated as fully verified for this work's prerequisite scope.
- [Protocol checklist, SGR requirements](../../docs/protocol/checklist.md#16-sgr-character-attributes).
- [Kitty colored and styled underline specification](https://sw.kovidgoyal.net/kitty/underlines/), also available as [upstream source](https://github.com/kovidgoyal/kitty/blob/master/docs/underlines.rst): style codes, SGR 58/59, explicit-color inverse behavior, and `Su` discovery. This reference does not imply Kitty identity or other Kitty support.
- Current integration surfaces: `crates/harbor-terminal/src/model.rs`, `screen/edit/pen_state.rs`, `normal_buf.rs`, `primary_reflow.rs`, `render/decoration.rs`, and `parser/status_strings.rs` (paths after the first are relative to the same terminal `src/` directory).

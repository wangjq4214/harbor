# Modern SGR Decorations

**Spec ID:** 0018
**Status:** Accepted (owner-confirmed)
**Date:** 2026-10-09
**Sources:** [Issue #162](https://github.com/wangjq4214/harbor/issues/162), the user-approved modern SGR refinement contract below, [ADR 0051](../adr/0051-modern-sgr-decoration-and-blank-retention-policy.md), and the approved Windows ConPTY live-primary exception in [ADR 0052](../adr/0052-conpty-live-primary-producer-geometry-and-styled-tail-clipping.md).

## Scope and Settled Contract

Deliver the modern SGR slice of #162 with Neovim as the first named workload: styled/colored underlines, conceal/reveal and overline. [ADR 0052](../adr/0052-conpty-live-primary-producer-geometry-and-styled-tail-clipping.md) governs the approved live-primary resize exception; unrelated work is not included.

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
- Retained history and non-ConPTY primary narrowing/widening preserve decorated blanks and their new state. For Windows ConPTY producer-owned live primary only, [ADR 0052](../adr/0052-conpty-live-primary-producer-geometry-and-styled-tail-clipping.md) takes precedence: a style-only trailing blank suffix (printed or styled erase/fill) must not create extra rows; preserve its attributes within available target-row capacity and clip/discard overflow, without a promise of later restoration or copy/selection. Preserve significant interior spaces, actual text, explicit hard blank lines, wide-cell invariants and producer-compatible live/saved cursor and pending-wrap semantics. Conceal's temporary visibility suppression must not classify hidden source text as disposable styled blank capacity.
- Producer geometry includes already soft-wrapped (`wrapForced`) old source prefixes, even if their final cells are ordinary or decorated blanks. Only the final old source row's style-only tail is clipped; do not drop forced-prefix geometry and thereby displace cmd repaint. See the measured producer profiles in ADR 0052.
- Preserve [ADR 0046](../adr/0046-trim-ordinary-trailing-spaces-during-primary-reflow.md): ordinary default-style, unprotected, non-hyperlinked logical tail spaces can still be discarded, subject to live/saved cursor exceptions. Do not turn all unused cells into retained decorated content or change unrelated output/erase semantics.
- Primary reflow and alternate-screen rectangular resize remain distinct. Apply the ADR 0052 live/history boundary also to saved-primary resize while alternate is active; do not change alternate rectangular policy or pull history into producer live coordinates. Wide-cell normalization, content anchors, selection projection, history eviction, and transactional resize continue under [spec 0014](./0014-content-preserving-resize-reflow.md). Anchors must reflect surviving cells; endpoints in clipped overflow clamp or invalidate consistently, without implying full tail preservation.

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
- Keep self-contained validation summaries using Validation, with raw artifacts retained locally or linked from retrievable CI. Record revision/dirty scope, Windows/ConPTY/application versions, commands/steps, expected/observed results, PASS/FAIL/NOT RUN/BLOCKED, and exclusions. Do not capture secrets or unrelated terminal/clipboard contents.
- Update protocol/status documentation only for behavior actually delivered and evidenced. The approved #162 prerequisites remain outside this slice's revalidation scope.

## Verification

Verify focused protocol/model and CPU/GPU behavior, plus configured Windows Neovim/synthetic scenarios, within the scope and exclusions above. Skipped GPU/native checks are not passes. Apply the standard quality gates at implementation boundaries and record omissions.

## Out of Scope

- OSC 4/104 palette operations, new clipboard behavior, legacy/horizontal/pixel mouse extensions, box-drawing geometry, Kitty keyboard/graphics, and complete historical xterm coverage.
- Changing `TERM`, DA identity, SSH/tmux deployment contracts, or claiming a transport not exercised by this slice.
- New rapid-blink behavior, complex shaping/ligatures, strikethrough-on-space expansion, configuration hot reload, or a font-management redesign.
- Conceal-based redaction, global selection/copy trimming, or broader reflow/alternate redesign beyond the approved Windows ConPTY live-primary style-only tail exception; preservation of every ordinary tail space remains out of scope.
- Reimplementation/revalidation of the approved completed #162 sub-issues as prerequisites.

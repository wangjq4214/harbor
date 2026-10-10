# Conceal and Overline

**Ticket ID:** T0002
**Source:** [Spec 0018 R3-R7](../../spec/0018-modern-sgr-decorations.md), [ADR 0051](../../adr/0051-modern-sgr-decoration-and-blank-retention-policy.md).
**Status:** Done

## Goal

SGR conceal and overline behave as independent presentation attributes without losing original text, widths, background, selection/copy meaning, or terminal state lifetime.

## Affected Surfaces

- `crates/harbor-terminal/src/model.rs`, `screen/edit/pen_state.rs`, writer/edit constructors and saved/reset state.
- `normal_buf.rs`, `logical_content.rs`, `primary_reflow.rs`, `content_anchor.rs`: decorated blank significance and retained/copy semantics.
- `render/text.rs`, `decoration.rs`, `background.rs`, `pipeline.rs`, `layout.rs`: foreground suppression, overline geometry, effective colors and invalidation.
- `screen.rs::current_sgr` and `parser/status_strings.rs`: accurate conceal/overline status.
- Focused core/parser/model/copy/reflow/CPU-geometry/GPU tests and scoped synthetic Windows evidence.

## Approach

Add conceal and overline to the existing authoritative pen/cell path and propagate through all existing constructors, editing, snapshot, save/reset and buffer operations. Keep semantic state GPU-independent.

Suppress only the foreground glyph/decorations of concealed cells in projection; never replace source text or mutate width/content to hide it. Reuse effective foreground resolution for overline. Paint overline on spaces and recognize these visible blanks in retained provenance/extents.

Extend the existing DECRQSS serializer for these attributes in the same delivery. If T0001 has not landed, prove conceal against current single underline/strikethrough and overline; T0003 covers combined modern-style/color interactions after integration.

## Dependencies and Coordination

- **Blocked by:** None. This behavior can be implemented and verified using current terminal contracts independently of modern underline styles.
- **Blocks:** T0003, because integrated acceptance requires conceal/overline.
- **Coordination risks:** T0001 touches the same cell/pen/constructor/provenance/decoration/status paths. Use the agreed spec and preferably serial editing; shared files alone are not a semantic blocker. Neither ticket may introduce a second owner or silently change the other's resets.

## Acceptance

- [x] `8`/`28` enable/disable conceal and `53`/`55` enable/disable overline for subsequent writes. Existing cells retain stored attributes and attribute-specific resets do not reset unrelated styling.
- [x] Concealed plain/CJK/suffix-bearing cells retain exact source, assigned width, background and cursor progression; selecting/copying them returns the original fixture text.
- [x] Conceal produces no foreground glyph, underline (including hyperlink fallback), strikethrough or overline, including under inverse. Modern explicit underline-color/style combinations are additionally verified by T0003.
- [x] Overline paints glyphs and spaces using effective text foreground, coexists with existing decorations, covers complete wide units, remains aligned/clipped through existing viewport/font/DPI transitions, and invalidates stale retained geometry.
- [x] Visibly overlined spaces, including those produced by existing styled erase/fill semantics, have correct meaningful extents and survive primary reflow. Conceal does not turn retained styled contents into disposable blank capacity.
- [x] Ordinary default-tail trimming and cursor exceptions remain intact; no change to rectangular alternate-screen resize, buffer isolation, content anchors, history eviction, or copied text beyond the selected attributes.
- [x] Writing, editing/moving, scrolling, saved-pen restoration, wide continuations and snapshots preserve the attributes; SGR reset, DECSTR and RIS restore defaults within existing operation semantics.
- [x] Fragmented/malformed/cancelled input, bounds, mixed attributes, reset and subsequent text recovery have focused deterministic tests.
- [x] DECRQSS accurately reports conceal/overline, preserves default/legacy bytes, round-trips the relevant pen state and keeps existing framing/cancellation/reply bounds.
- [x] Focused/core/CPU-geometry/GPU tests and scoped synthetic Windows rendering/copy/resize evidence are recorded with versions, revision/dirty scope and honest results/exclusions. Conceal is documented as presentation, not redaction.
- [x] Existing hyperlink, cursor/selection, IME and input contracts are preserved, and applicable implementation gates are run or explicitly marked unrun with reasons.

## Implementation evidence

The T0002 state/projection/status implementation was delivered in `0db427a`. Its scoped core/workspace/CPU/GPU and Windows rendering/original-text copy evidence is supplemented by the completed cmd resize repair in `cb6ee0a` / [ADR 0052](../../adr/0052-conpty-live-primary-producer-geometry-and-styled-tail-clipping.md). The earlier partial-input redraw anomaly is resolved, not a current blocker. [T0003 integrated acceptance](../../../docs/modern-sgr-acceptance.md) refreshes workspace/GPU, combined modern-style/color status and suppression, native original-text copy and pending-input resize evidence. Full decorated-tail retention remains history/non-ConPTY behavior; live ConPTY overflow clipping follows ADR 0052. Native font-settings reload, cross-monitor DPI transitions and IME smoke remain NOT RUN; observed 1.5x native rendering and automated resource/input regressions are not those passes. Done denotes this feature scope, not broad release acceptance.

## Out of Scope

New underline styles/color (T0001), Su/final Neovim acceptance (T0003), security redaction, changed copied text, expansion of strikethrough-space rendering, a font/reflow/cursor/selection/IME redesign, new clipboard policy, and unrelated #162 extensions.

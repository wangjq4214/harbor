# Proportion-Preserving Nerd Font Icon Presentation

**Status:** Superseded
**Superseded by:** [0092-natural-size-clipped-nerd-font-icons](./0092-natural-size-clipped-nerd-font-icons.md)
**Date:** 2026-10-26
**Sources:** Refinement conversation: the user requested natural-proportion Nerd Font icons that may extend into adjacent blank cells without covering text, then approved borrowing following spaces and uniformly downscaling when space is insufficient.

**Supersession note (2026-10-26):** The user subsequently approved natural-size artwork with clipping at unavailable boundaries instead of uniform shrink-to-fit. ADR 0092 carries forward the unchanged icon scope, borrowing boundaries and logical text constraints. The body below preserves the historical decision and its original implementation-status statements.

## Context

The ordinary terminal glyph path in `crates/harbor-terminal/src/render/text.rs` currently reduces oversized artwork to its assigned width without reducing its height. This distorts wide Nerd Font icons, even when following cells are blank.

The user approved a presentation-policy direction, not an implementation. This record is Proposed because no implementation or runtime verification has been performed for this change.

## Decision

- Apply this policy to the official Nerd Fonts icon collection, not every private-use character or every oversized ordinary glyph. Preserve icon aspect ratio; do not fit oversized icons by reducing horizontal dimensions alone.
- Allow icon artwork to use following ordinary blank cells, aiming to retain its natural size when space permits. The user subsequently rejected borrowing special/styled blanks, including spaces with a different background; treat these as an unavailable boundary and uniformly downscale to the available region instead.
- Do not borrow blank cells carrying hyperlinks or protection markers, even when they look empty. Ordinary blanks occupied by the cursor, selection or IME preedit also block borrowing. These three eligibility boundaries were explicitly approved in the final refinement turn.
- When available space is insufficient, uniformly downscale the icon rather than horizontally squeezing it. Cropping at natural size is not the selected primary fit policy.
- Do not cover, compress, or push neighboring text to accommodate an icon.
- Keep this a presentation change: visual use of blank cells does not allocate extra logical columns or change retained text, cursor advancement, selection/copy, or wrapping semantics.

## Consequences

- Aspect-ratio preservation does not promise identical icon size in every context: insufficient space can reduce both width and height.
- The presentation must respond when neighboring content changes so previously usable space does not remain painted over new text.
- Presentation eligibility must use the approved official-icon scope and conservative blank/overlay boundaries, not infer free space solely from a visually empty cell or reuse the logical meaningful-blank/reflow classification as a competing policy.
- This policy is scoped to the requested Nerd Font icon behavior; it does not authorize changing every oversized ordinary glyph.
- The shared text/grid-width semantics of [ADR 0044](./0044-unicode-text-unit-and-presentation-width-policy.md) remain authoritative. [Spec 0044](../spec/0044-proportion-preserving-nerd-font-icons.md) is the selected short requirements artifact. It narrowly qualifies [Spec 0015](../spec/0015-unicode-terminal-text-correctness.md)'s assigned-cell presentation wording for eligible Nerd-icon artwork, not logical width. The spec-only endpoint does not authorize implementation or ticket slicing.
- [ADR 0054](./0054-color-emoji-offscreen-presentation.md)'s emoji fitting policy is unchanged. This decision does not select a renderer architecture or claim implementation completion.

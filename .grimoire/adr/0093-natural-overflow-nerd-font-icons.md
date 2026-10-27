# Natural Overflow for Nerd Font Icons

**Status:** Testing
**Date:** 2026-10-27
**Supersedes:** [0092-natural-size-clipped-nerd-font-icons](./0092-natural-size-clipped-nerd-font-icons.md)
**Sources:** User refinement: “现在这个图标还是不理想……改成不裁剪，重叠就让他重叠吧，让图标可以自然溢出就行了”.

## Context

ADR 0092 retained natural-size artwork but clipped it to its assigned cells and following eligible blanks. The user remains dissatisfied and now explicitly prefers natural overflow, accepting overlap rather than clipping icons to protect neighboring content.

## Decision

- Retain the official Nerd Fonts icon scope and natural artwork size/proportions at the current font, font size and DPI. Private-use membership alone does not select this presentation policy.
- Let icon artwork naturally extend outside its logical cells. Do not clip it to its allocated cells or to a region computed from following eligible blanks; do not shrink, horizontally squeeze or arbitrarily enlarge it to fit.
- Neighboring text, styled/different-background blanks, hyperlinks/protection and cursor/selection/IME occupancy no longer impose an icon-specific clipping boundary. Visual overlap is accepted rather than prevented by clipping or fitting.
- Keep logical allocation and retained content unchanged: do not move or compress neighboring text, add columns/spaces, or change cursor advancement, wrapping, reflow, selection or copied text to accommodate the artwork.
- Ordinary text and emoji presentation policies remain unchanged. This decision changes Nerd-icon presentation, not the terminal model.

## Consequences

- Natural-size artwork is preferred over the previous no-overlap guarantee. Overlap can reduce the readability of neighboring content; the user explicitly accepts this tradeoff.
- Conservative blank borrowing and occupancy-driven clipping from ADR 0092 are no longer the selected icon policy. Shrink-to-fit from ADR 0091 remains superseded.
- **Documentation alignment (2026-10-27):** [Spec 0044](../spec/0044-proportion-preserving-nerd-font-icons.md) now expresses the natural-overflow contract, with the related presentation qualification aligned in [Spec 0015](../spec/0015-unicode-terminal-text-correctness.md). Historical clipping evidence is retained and labeled separately; it does not establish implementation or acceptance of natural overflow. This documentation update does not change the decision.
- Natural overflow is implemented with focused geometry/native GPU regression and terminal-suite evidence in [Spec 0044](../spec/0044-proportion-preserving-nerd-font-icons.md#natural-overflow-implementation-and-verification-2026-10-27). Broader application/font acceptance and reproduction of the user's original workload are not established.

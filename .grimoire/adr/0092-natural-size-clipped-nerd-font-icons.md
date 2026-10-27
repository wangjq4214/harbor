# Natural-Size Nerd Font Icons with Boundary Clipping

**Status:** Superseded
**Superseded by:** [0093-natural-overflow-nerd-font-icons](./0093-natural-overflow-nerd-font-icons.md)
**Date:** 2026-10-26
**Supersedes:** [0091-proportion-preserving-nerd-font-icons](./0091-proportion-preserving-nerd-font-icons.md)
**Sources:** Follow-up refinement conversation: the user reported that the current icons appear very small, requested comparison with Windows Terminal, and approved retaining natural size with boundary clipping instead of shrinking the complete icon when space is insufficient.

**Supersession note (2026-10-27):** The user rejected icon clipping and accepted natural overflow even when it overlaps neighboring content. ADR 0093 replaces the clipping and no-overlap decisions. The body below preserves the historical policy and verification statements.

## Context

ADR 0091 selected uniform downscaling when the icon's assigned cells and following eligible blanks cannot contain its natural artwork. This preserves shape and completeness, but a wide icon confined to a narrow region also loses height. The user now prefers a partially clipped natural-size icon to a tiny complete icon.

The selected approach borrows the separation of logical column allocation from visible artwork size, not unrestricted overflow onto neighboring text. The earlier official-icon scope, blank eligibility and logical text constraints remain unchanged.

## Decision

- Apply this policy to the official Nerd Fonts icon collection, not every private-use character or every oversized ordinary glyph.
- Render icon artwork at its natural size for the current font, font size and DPI, preserving its proportions. Allow it to extend rightward into contiguous following eligible ordinary blank cells on the same displayed row.
- When the available region is insufficient, clip the artwork at the unavailable boundary or displayed-row bounds instead of shrinking the complete icon to fit. Do not horizontally squeeze it or enlarge it with an arbitrary multiplier.
- Retain the existing conservative borrowing boundaries: actual text, special/styled blanks (including different-background spaces), hyperlinked/protected blanks and blanks occupied by the cursor, selection or IME preedit are unavailable. Retained hidden text, combining content and wide-character continuation cells are not free blanks. Do not skip a blocked cell to reach later blanks.
- Do not cover, compress or move neighboring text. Visual use of blank cells does not allocate logical columns or change retained content, cursor advancement, wrapping, reflow, selection or copied text.
- Reevaluate the visible region when neighboring content or overlay occupancy changes; font, DPI and viewport changes use current geometry. No stale artwork may remain over a newly blocked cell.
- Ordinary text and emoji policies are unchanged. This is a Nerd-icon presentation policy, not a terminal-model or renderer-architecture replacement.

## Consequences

- Tight layouts may show an incomplete icon. The user explicitly accepts that tradeoff in preference to a uniformly shrunken complete icon.
- Sufficient eligible space must retain natural-size artwork; clipping does not authorize treating eligible ordinary blanks as unavailable merely to keep artwork inside its logical columns.
- Shrink-to-fit and unrestricted overflow onto neighboring text are not the selected policies. Copying Windows Terminal's full rendering behavior is not a requirement.
- **Documentation correction (2026-10-26):** [Spec 0044](../spec/0044-proportion-preserving-nerd-font-icons.md) now expresses this natural-size clipping contract, with the related presentation-only qualification aligned in [Spec 0015](../spec/0015-unicode-terminal-text-correctness.md). This replaces the initial recording's pending-spec wording without changing the decision. These requirements artifacts do not establish implementation or runtime acceptance.
- The clipping policy is implemented with focused automated and Windows GPU regression evidence in [Spec 0044](../spec/0044-proportion-preserving-nerd-font-icons.md#implementation-and-verification-2026-10-26). Broader application/font acceptance is not established. The user's original small-icon workload has not been independently reproduced; conservative blank detection remains a candidate cause rather than a confirmed diagnosis.

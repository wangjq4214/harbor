# Font Reload Uses Normal Bounded Resize

**Status:** Proposed
**Date:** 2026-10-14
**Sources:** User clarification during issue #172 refinement: after changing terminal font size, render at the new size and keep the newest content; do not conservatively reject reload merely to retain every older cell or history row.
**Supersedes in part:** [ADR 0059](./0059-atomic-manual-reload-and-terminal-only-fonts.md), only its unqualified retained-content preservation requirement during font-driven grid changes. Application/session/PTY continuity, terminal-only font scope, atomic configuration acceptance, and OSC precedence remain applicable.

## Context

The earlier request that content must not be lost was recorded without distinguishing session reset from ordinary bounded resize behavior. Font metrics can change the terminal's row and column counts. Existing primary reflow applies a physical-row capacity and keeps newer rows when capacity is exceeded; alternate-screen resize is rectangular and may clip out-of-bounds cells. A conservative loss-detection rejection was proposed during discussion but was not approved.

## Decision

Terminal font family and size changes use normal terminal geometry, reflow, and rendering behavior. Preserve the existing session, PTY, and content model rather than clearing buffers or constructing a fresh terminal. The newest content is retained according to the existing bounded resize policies; older history may be evicted when reflow exceeds capacity. Alternate-screen resizing retains its existing rectangular policy, including boundary clipping and application-driven redraw. The existing ConPTY live-primary compatibility policy is not replaced.

Do not reject an otherwise valid font reload solely because normal resize would evict old history or clip cells. This scope does not require permanent retention of every historical row, hidden off-grid backups, or a separate lossless font-resize model. External UI font policy remains independent. Validation and actual resource/preparation failures still reject reload under ADR 0059; accepting ordinary bounded retention does not authorize silent partial configuration acceptance.

## Consequences

- The continuity promise is no application/PTY restart, session recreation, or buffer reset, not unlimited historical-content preservation.
- Font-driven resize reuses existing bounded main/alternate/ConPTY behavior instead of introducing a special conservative rejection rule.
- This clarification establishes the intended contract, not implementation or runtime acceptance of font reload.

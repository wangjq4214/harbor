# Modern SGR Decorations

**Source:** [Spec 0018](../../spec/0018-modern-sgr-decorations.md), [ADR 0051](../../adr/0051-modern-sgr-decoration-and-blank-retention-policy.md), and [#162](https://github.com/wangjq4214/harbor/issues/162).
**Ticket folder:** `.grimoire/ticket/0015-modern-sgr-decorations/`
**Status:** Ready for execution handoff; implementation has not started.

## Overview

Deliver the user-approved modern SGR slice with Neovim as the named application workload. Preserve the separation between logical state and renderer projection, existing reset/save/buffer semantics, ordinary-tail trimming, hyperlink behavior, and truthful bounded replies.

All existing #162 sub-issues (#173 and #174 at refinement) are treated as complete and verified by user direction. They are not blockers or new work in this set. Remaining palette/mouse/box-drawing and broader transport work belongs to the parent tracker.

The approved spec is authoritative. This set organizes settled requirements; it does not select new protocol semantics, thresholds, architecture, or private representation. A needed new material choice returns to refinement before implementation or artifact revision.

## Delivery Surfaces

- Terminal pen, cell, saved state, writer/edit operations, wide continuation and reset paths.
- Normal-buffer blank provenance, meaningful extents, logical content, reflow, content anchors and copy.
- Text/decoration rendering, effective color, metrics, palette/viewport invalidation and retained GPU projection.
- Existing DECRQSS status serialization, XTGETTCAP registry, and bounded TerminalReply transport.
- Focused/core/GPU tests, Windows synthetic VT and Neovim evidence, protocol/status documentation.

There is no standalone parser/model pre-refactor ticket: each state extension belongs to its first complete visible consumer.

## Requirement Coverage

| Spec requirement | Responsible tickets |
| --- | --- |
| R1 styles and legacy compatibility | T0001 |
| R2 independent underline color | T0001 |
| R3 explicit underline geometry/spaces and hyperlink fallback | T0001 |
| R3 overline and coexistence | T0002; cross-feature checks in T0003 |
| R4 conceal/reveal and preserved source/copy | T0002; new-style/color interaction in T0003 |
| R5 state lifetime, provenance and resize | T0001/T0002 for their attributes; integrated regressions in T0003 |
| R6 DECRQSS | Each producer includes its own new state; combined exact/round-trip checks in T0003 |
| R6 Su discovery, unchanged TERM/identity | T0003 |
| R7 named application, regressions and final evidence/documentation | T0003; each producer supplies focused and scoped runtime evidence |

## Dependencies

| Producer | Consumer | Why consumer cannot complete first |
| --- | --- | --- |
| T0001 | T0003 | Truthful Su advertisement and Neovim acceptance require implemented and verified styled/color underlines. |
| T0002 | T0003 | The final delivered SGR matrix, combined status and conceal/overline interactions cannot be accepted before those attributes exist. |

T0001 and T0002 have no mutual semantic blocking edge. T0002 can prove conceal against existing single underline/strikethrough and overline without waiting for modern underline forms. T0003 verifies their combined state. Its fixtures/procedures may be prepared earlier, but final completion requires both producers.

## Coordination

| Tickets | Risk | Strategy |
| --- | --- | --- |
| T0001 / T0002 | Shared `model.rs`, `pen_state.rs`, writers, provenance, `decoration.rs`, `status_strings.rs` and tests | Prefer sequential edits. If parallel, coordinate one authoritative state representation, resets, constructors, and serializer integration; do not add duplicate legacy/new underline ownership. |
| Producers / T0003 | Premature capability claims or documentation checkmarks | Keep Su addition/evidence ownership in T0003; producers publish only their exact scoped results. |
| All | Renderer and reflow disagree about decorated blanks; hidden/failed draw loses new damage | Reuse spec R3-R5 semantics and existing update/acknowledgment contract; final combined regression checks cover these boundaries. |

## Recommended Order

1. T0001: complete styled/color underlines, including state lifetime, blank retention, rendering and status.
2. T0002: complete conceal/overline and their state/render/status effects.
3. T0003: integrate verified capability discovery, run the combined Windows/Neovim matrix and publish scoped acceptance.

The first two can proceed with coordinated parallel work, but shared-file risk makes sequential execution the default recommendation, not a semantic dependency. Do not start a separate plan stage or treat this README as an execution result.

## Ticket Index

| Ticket | File | Outcome |
| --- | --- | --- |
| T0001 | [Styled and colored underlines](./T0001-styled-colored-underlines.md) | Complete underline style/color rendering and retention with accurate status |
| T0002 | [Conceal and overline](./T0002-conceal-overline.md) | Independent presentation attributes with preserved original content |
| T0003 | [Capability discovery and Neovim acceptance](./T0003-capability-neovim-acceptance.md) | Truthful Su discovery and evidenced integrated SGR delivery |

## Handoff and Permissions

- Selected endpoint: spec plus ticket set only. No production changes, native test execution, GitHub issue creation, staging, or commit is authorized by artifact authoring.
- Runtime evidence for this new slice must be newly observed during execution. The prerequisite-completion premise does not fabricate new passes or alter existing historical records.
- Private field layout, shader/geometry strategy and canonical new-state serialization ordering are implementation details within the spec; additional colorspace protocols and material policy changes are not.
- Existing non-SGR #162 outcomes are excluded, not completed by this ticket set.
- Once explicitly invoked for implementation, `grimoire-loop` can execute this set starting with T0001.

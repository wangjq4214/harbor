# Styled and Colored Underlines

**Ticket ID:** T0001
**Source:** [Spec 0018 R1-R3, R5-R7](../../spec/0018-modern-sgr-decorations.md), [ADR 0051](../../adr/0051-modern-sgr-decoration-and-blank-retention-policy.md).
**Status:** Done

## Goal

Applications can emit the accepted explicit underline styles and colors and see them rendered correctly, including spaces and wide text. The state survives existing terminal operations and resize and is represented accurately by DECRQSS.

## Affected Surfaces

- `crates/harbor-terminal/src/model.rs`: authoritative semantic cell/snapshot state and blank classification.
- `screen/edit/pen_state.rs`, `cell_writer.rs`, `cell_ops.rs` (relative to terminal `src/`): SGR, defaults/reset, saved pen, constructors and editing.
- `normal_buf.rs`, `logical_content.rs`, `primary_reflow.rs`, `content_anchor.rs`: provenance, meaningful extent, retained state and projection.
- `render/decoration.rs`, `text.rs`, `pipeline.rs`, `layout.rs`; font metrics where necessary: decoration geometry, effective color and invalidation.
- `screen.rs::current_sgr` and `parser/status_strings.rs`: new underline state exposed to existing bounded status replies.
- Focused parser/model/reflow/CPU-geometry/GPU tests and scoped synthetic Windows evidence.

## Approach

Extend the current Params -> pen -> cells -> snapshot -> renderer path as one complete consumer. Distinguish all approved styles and independent underline color, including saved state and wide continuation cells. Keep one authoritative style rather than an unsynchronized flag plus separate style.

Resolve default underline color through the effective foreground and explicit colors through the existing palette path. Draw explicit decorations across spaces and complete wide units; use current metrics/viewport projection and invalidation. Private storage, geometry/shader strategy and shared parsing helpers remain implementation details.

Maintain both meaningful blank provenance and ordinary-tail trimming. Current explicit underlined spaces are already protected by the non-default-cell check; focus additionally on styled erase/fill extents and newly carried state.

Extend DECRQSS alongside the state producer so it does not silently report modern underlines as legacy single. Do not add Su or unrelated capability/identity claims in this ticket.

## Dependencies and Coordination

- **Blocked by:** None. Existing #162 sub-issues are accepted complete.
- **Blocks:** T0003, because Su and Neovim acceptance require this delivered support.
- **Coordination risks:** T0002 shares state, constructors, reset, provenance, renderer and serializer files. Prefer sequential editing; parallel work must agree on authoritative state and avoid divergent helper/field ownership. Conceal interactions with modern styles/colors are checked together in T0003.

## Acceptance

- [x] `4:0..5`, legacy `4`/`24`, and `21` produce the specified styles; `21` does not clear bold and unknown styles retain previous style.
- [x] All selected SGR 58 indexed/RGB semicolon/colon forms, including empty colorspace RGB, work within `0..255`; incomplete/out-of-range candidates leave previous underline color unchanged.
- [x] Color changes do not enable style; `24`/`4:0` preserve color, re-enable reuses it, and `59` restores effective-foreground following. Explicit colors do not swap under inverse.
- [x] Mixed top-level/colon parameters, ordered resets, every relevant fragmentation boundary, malformed/cancelled CSI, parser bounds and subsequent text recover correctly without subparameters leaking into other attributes.
- [x] Written cells, wide continuations, edited/moved cells, scrollback, saved pen, buffer transitions, SGR reset, DECSTR and RIS carry/reset the new state according to spec R5.
- [x] CPU geometry and GPU evidence distinguish all styles, preserve contiguous decoration over spaces/adjacent cells, cover both halves of wide text without duplicate artifacts, and respect viewport/font/DPI invalidation and clipping.
- [x] Explicit decorated tails and existing styled erase/fill blanks survive primary narrowing/widening with correct meaningful extents. Default undecorated tails, live/saved cursor exceptions, capacity eviction and selection/copy anchor behavior retain existing rules.
- [x] Alternate-screen resize remains rectangular, saved primary reflow retains the new state, and no primary/alternate state leaks.
- [x] Non-space OSC 8 fallback remains single when explicit style is off, explicit enabled style wins, and linked spaces do not acquire fallback decoration. Existing link lifetime/activation is unchanged.
- [x] DECRQSS includes accurate underline style/color, preserves legacy/default exact bytes, round-trips into the same pen state, and retains framing/cancellation/bounds.
- [x] Focused tests and scoped synthetic Windows rendering/resize evidence record commands, revision/dirty scope, versions, observations and exclusions under the validation policy. No Neovim or broader transport pass is inferred from these alone.
- [x] Applicable implementation gates are run with unrun commands explained. Documentation records only the exact evidenced underline scope; Su remains T0003's responsibility.

## Delivery Evidence

Implemented against baseline `84bed2aaf0a7ea4ffa36ccaf86dea13d6bcc1988` plus the T0001 dirty-tree scope. [Acceptance mapping, commands, GPU readback and synthetic Windows resize observations](../../../docs/verification/modern-underlines-t0001.md) establish the scoped result. Workspace: 2269 passed, 5 ignored; core-only terminal: 669 passed, 2 ignored. Ignored/manual gates are not passes. T0002/T0003, Su, Neovim, broader transport and release acceptance remain outside this delivery.

## Out of Scope

Conceal/overline implementation (T0002), Su discovery and final Neovim acceptance (T0003), a standalone parser/model pre-refactor, OSC palette mutation, new TERM/identity, rapid blink, expanded strikethrough-space semantics, complete historical SGR coverage, and new reflow algorithms.

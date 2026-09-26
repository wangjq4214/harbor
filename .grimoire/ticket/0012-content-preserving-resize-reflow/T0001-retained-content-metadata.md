# Retained Content Metadata

**Ticket ID:** T0001
**Source:** [Spec 0014](../../spec/0014-content-preserving-resize-reflow.md), GitHub #169
**Status:** Todo

## Goal

Make logical-line identity and meaningful retained content explicit in the bounded normal-screen ring so every write, erase, edit, scroll, and reset path leaves metadata that later copy, anchor, and reflow work can trust.

## Affected Surfaces

- **Normal buffer:** Replace the standalone wrapped-row representation with row metadata carrying logical-line identity, logical start, meaningful extent, soft-wrap state, and truncated-head state.
- **Screen write/edit paths:** Maintain metadata through printing, ordinary spaces, pending-wrap transitions, erase, insertion/deletion, line movement, scrolling, rectangular operations, and reset.
- **Cell semantics:** Distinguish printed and visibly styled/hyperlinked blanks from unused default capacity without changing the bounded ring architecture.
- **Invariant tests:** Extend normal-buffer and screen editing fixtures to assert identity non-reuse, metadata movement, and wide-cell consistency.

## Approach

Introduce crate-internal `LogicalLineId`, row metadata, and the minimal cell/row occupancy representation required by ADR-0042. Allocate identities monotonically and never derive them from ring slots or physical generations. Soft-wrap continuation rows share identity and carry cumulative logical offsets; explicit newline and independent-row clearing establish new identities. Move metadata atomically with row content in structural operations.

Preserve ADR-0016 wide-cell normalization and ADR-0035 hyperlink ownership. Default-style erase removes meaningful content; visibly styled erase remains meaningful. This ticket establishes model truth but does not switch resize to reflow or replace selection coordinates.

## Dependencies and Coordination

- **Blocked by:** None; this is the executable #169 foundation.
- **Blocks:** T0002, because logical decoding and copy need authoritative identity and extent metadata.
- **Coordination risks:** Broad overlap in `normal_buf.rs`, `screen/edit/*`, writer, reset, and screen tests. Complete metadata invariants before downstream tickets alter copy or resize.

## Acceptance

- [ ] Every retained row has a valid logical-line identity, logical starting offset, meaningful extent, soft-wrap state, and non-truncated default state.
- [ ] Explicit newline creates a new identity; soft wrap shares identity; ring-slot reuse never reuses an old logical identity.
- [ ] Printed default spaces and visibly styled/hyperlinked blanks remain meaningful, while unused capacity and default-style erase do not.
- [ ] Character edits adjust extent without replacing line identity; structural row operations move cell and row metadata together.
- [ ] Reset and alternate-screen creation establish independent fresh identity spaces.
- [ ] Existing wide-cell, protection, margin, damage, scrolling, and hyperlink invariants continue to pass focused tests.
- [ ] Current production resize remains the documented ADR-0018 non-reflow behavior at the end of this ticket.

## Out of Scope

- Logical atom decoding or replacing `selected_text` trimming.
- Public content-anchor APIs or selection migration.
- Production reflow, PTY resize sequencing, or runtime acceptance claims.

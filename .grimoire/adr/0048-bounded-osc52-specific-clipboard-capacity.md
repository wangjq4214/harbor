# Bounded OSC 52-Specific Clipboard Capacity

**Status:** Proposed
**Date:** 2026-09-30

## Context

The existing generic OSC retention cap is 4096 bytes, allowing only roughly 3 KiB of base64-decoded clipboard text and making larger remote-editor selections impractical. During #174 refinement, the user accepted a larger bounded capacity, selected 4M instead of the suggested 1 MiB, and agreed not to add capacity configuration; the discussion uses binary MiB units.

## Decision

The delivered OSC 52 write subset accepts at most 4 MiB (4,194,304 bytes) of decoded UTF-8 text, with a fixed limit and no user-facing capacity setting. Only OSC 52 receives a larger retention allowance; other OSC and string-family limits remain unchanged, and buffers grow with actual input rather than preallocating the maximum for every session.

## Consequences

- Base64 data is bounded at 5,592,408 bytes (four times the ceiling of 4,194,304 divided by three); protocol framing fields need separately bounded overhead.
- Enforce both encoded and decoded limits before unbounded retention or allocation. The encoded bound alone does not prove that decoded data fits.
- Reject a whole over-limit request without copying a truncated prefix; preserve correct BEL/ST, cancellation/reset, fragmented-input, and subsequent-text behavior.
- Larger single requests do not authorize unbounded queues or confirmation retention; aggregate pending-work bounds still need to be specified.
- This supplements [ADR 0047](./0047-foreground-only-host-authorized-osc52-writes.md); foreground permission and live-session checks are not relaxed by the larger capacity.

## Sources

- User refinement discussion (2026-09-30): larger buffer requested, then 4M selected with no capacity configuration.
- `.grimoire/CONTEXT-terminal-protocol.md`: Parser Retention Limits and OSC 52 Host-Authorized Clipboard Write.
- `crates/harbor-parser/src/params.rs`: existing MAX_OSC_BYTES and MAX_STRING_BYTES are 4096.

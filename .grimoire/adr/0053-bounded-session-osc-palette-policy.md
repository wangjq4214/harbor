# Bounded Session-Owned OSC Palette Policy

**Status:** Completed
**Date:** 2026-10-10
**Sources:** [Issue #191](https://github.com/wangjq4214/harbor/issues/191) and the user's approval of the recommended palette contract on 2026-10-10. Approval establishes the contract, not implementation or runtime acceptance.

## Context

Harbor stores configurable ANSI normal/bright colors, but currently calculates indexed colors 16-255 during resolution. OSC 4/104 require all 256 indexed entries to be reprogrammable, queryable, and resettable, with query results matching rendered colors. The existing [OSC default-color policy](./0036-preserve-harbor-alpha-for-osc-default-colors.md) is deliberately narrow and preserves Harbor alpha. Its single-color commands must not acquire OSC 4's multi-entry semantics.

The material compatibility alternatives discussed were supporting special indexes such as -1 versus a 0-255 subset, accepting color names versus the existing RGB subset, and applying valid pairs from a malformed request versus validating the whole request first. The user selected the recommended bounded subset and whole-request validation.

## Decision

The terminal session owns one authoritative active indexed palette and its startup reset baseline. Only indexes 0-255 are supported; reject invalid indexes rather than clamp, grow storage, or alias special slots. ANSI named/bright colors and the corresponding indexed entries must resolve consistently. Existing cells retain semantic colors, so changing an entry affects already-written indexed content as well as subsequent output.

OSC 4 accepts the existing `#RRGGBB` and `rgb:R/G/B` subset with one to four hexadecimal digits per component; an exact `?` is a query, not a color name. Color names, `rgbi:`, and alpha-bearing forms remain unsupported. Sets replace RGB while preserving the active entry's alpha. Validate the complete request before any mutation or reply; a malformed pair or invalid index rejects the whole request. Execute valid set/query pairs in appearance order, including repeated indexes.

OSC 104 restores the complete startup RGBA value of selected indexed entries, or all indexed entries for an empty payload. Validate a nonempty reset list completely before changing state; repeated valid indexes are allowed. Palette resets do not reset default foreground/background, cursor, or selection colors. Startup ANSI entries come from the session's configuration; the remaining entries start with Harbor's existing color-cube/grayscale values.

Palette state is shared across the session's primary/alternate buffers, isolated between terminal sessions, and preserved through alternate-screen transitions, RIS, DECSTR, and SGR reset. Explicit OSC 104 restores the indexed palette. Existing OSC 10/11/12/110/111/112 behavior remains unchanged.

Queries use lowercase `rgb:rrrr/gggg/bbbb`, with the existing 8-bit RGB quantization/expansion and the request's BEL or ST terminator. Use [TerminalReply](./0017-platform-neutral-terminal-replies.md), retaining its 1024-byte cumulative bound and whole-reply acceptance/drop behavior. Whole-request validation is not an all-or-nothing reservation of reply-buffer capacity.

This extends indexed palette behavior without superseding ADR 0036's default-color contract. The concrete private storage representation is not selected here.

## Consequences

- Applications can reprogram the full indexed range without unbounded palette allocation or disagreement between query and display.
- Consumers of the coherent terminal update must see the same active palette; retained GPU colors and pending damage cannot remain stale after a palette change, under the [GPU-independent core boundary](./0045-gpu-independent-terminal-core-boundary.md).
- Whole-request rejection intentionally does not preserve otherwise-valid pairs from a malformed OSC sequence. Request ordering remains observable for valid mixed set/query operations.
- Resets honor startup configuration and preserve the separation between indexed colors and OSC default colors; RIS is not an implicit palette reset.
- Broader xterm color syntax, special slots, configuration hot reload, and changes to reply-buffer policy require separate contracts.

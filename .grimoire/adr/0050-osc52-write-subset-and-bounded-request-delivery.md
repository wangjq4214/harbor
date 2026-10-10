# OSC 52 Write Subset and Bounded Request Delivery

**Status:** Implementing
**Date:** 2026-09-30

## Context

Issue #174 deliberately delivers only a supported OSC 52 write subset; the 4 MiB capacity and optional asynchronous confirmation also require bounded pending-request behavior. The user confirmed the final protocol and delivery recommendations on 2026-09-30 before authorizing their inclusion in a single spec.

## Decision

Accept `c` or an empty selection as the system clipboard, valid UTF-8 text without NUL, and standard base64 with or without valid padding; reject other selections, malformed encoding, and binary/non-UTF-8 data. Empty decoded content requests clipboard clearing through the same permission checks, BEL/ST termination supports fragmented input, and OSC 52 reads remain denied.

In allow mode, retain only the latest unexecuted valid write for each originating session rather than accumulating a FIFO of clipboard payloads; this preserves the latest copy while bounding pending delivery. In confirm mode, preserve the exact admitted request and reject later requests while it is pending rather than replacing content the user is reviewing.

## Consequences

- An unsupported selection must not be silently mapped to the system clipboard; primary selection, cut buffers, multi-target selections, and URL-safe/non-standard base64 are outside the selected subset.
- Preserve ordinary text newlines and tabs; reject rather than truncate embedded NUL, invalid UTF-8, malformed base64, and capacity violations.
- The fixed limits from ADR 0048 apply before unbounded retention/allocation; empty clearing requests receive no authorization shortcut.
- Latest-write replacement remains scoped to a live originating session. Replacing a pending payload must not retarget another tab, bypass foreground/session checks, or alter unrelated terminal events.
- Confirmation retains at most one request under ADR 0049; extra requests cannot replace the previewed payload or open additional dialogs.
- Denial and failure use content-free diagnostics, no extra popup windows, no error text injected into terminal text, and no protocol error replies injected into the PTY. No success response or clipboard-read response is added by this delivery.
- This settles the previously open subset, pending delivery, and denial/failure UX questions in ADR 0049 without changing its permission policy.

## Sources

- User's final confirmation of the protocol subset, empty-write behavior, allow-mode coalescing, confirm-mode rejection, and content-free diagnostic-only UX (2026-09-30).
- [Issue #174](https://github.com/wangjq4214/harbor/issues/174): bounded writes, host effects, read denial, fragmented input, malformed/permission/session/runtime coverage, and no direct terminal clipboard API.
- [ADR 0048](./0048-bounded-osc52-specific-clipboard-capacity.md): fixed 4 MiB decoded capacity, no capacity configuration, and OSC 52-specific retention.
- [ADR 0049](./0049-osc52-confirmation-window-and-foreground-policy.md): foreground authorization and independent per-request confirmation.

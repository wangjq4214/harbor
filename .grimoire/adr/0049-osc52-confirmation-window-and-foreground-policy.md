# OSC 52 Confirmation Window and Foreground Policy

**Status:** Implementing
**Date:** 2026-09-30
**Supersedes:** [ADR 0047](./0047-foreground-only-host-authorized-osc52-writes.md)

## Context

ADR 0047 requires main-window focus for OSC 52 writes, but an independent confirmation window necessarily takes focus from the main window. Harbor already uses a separate native window for paste confirmation (ADR 0007); the user accepted the same interaction style for optional OSC 52 write confirmation rather than a main-window confirmation panel.

## Decision

Retain default allow for writes from the active live session while the Harbor main window has focus, configurable allow/deny/confirm behavior, host-owned clipboard effects, and no OSC 52 reads. In confirm mode, use an independent confirmation window and treat its own focus as a narrowly scoped continuation of the originating foreground request, not as background permission; require explicit allow-this-request or deny-this-request with no implicit permanent authorization.

## Consequences

- Terminal parsing emits typed requests without system clipboard I/O. The host preserves and revalidates the originating live session identity before applying the existing clipboard effect; it must never substitute whichever session is active later.
- Non-active sessions, stale sessions, and genuinely background or minimized application windows cannot write even under allow policy. The confirmation-window focus exception applies only to an already admitted foreground request, not to fresh requests from background sessions.
- Show the source tab, decoded text size, and a length-limited preview, not the full 4 MiB payload. The user grants permission only for the exact pending request.
- Retain at most one pending confirmation request at a time and reject new requests while confirmation is pending; do not replace its content or accumulate dialogs and payloads.
- Cancel the pending request if its source session closes, the active tab changes, or the user switches to another application. Moving focus to the request's confirmation window alone does not invalidate it.
- This follows the existing paste confirmation's independent-window interaction model; it does not authorize changing user-initiated paste semantics.
- The fixed 4 MiB decoded-text capacity, OSC 52-specific retention allowance, and no-capacity-configuration decision in [ADR 0048](./0048-bounded-osc52-specific-clipboard-capacity.md) remain unchanged.
- At the time of this confirmation decision, denial/failure UX, exact protocol subset, and non-confirmation request delivery remained unresolved. They were subsequently settled on 2026-09-30 in [ADR 0050](./0050-osc52-write-subset-and-bounded-request-delivery.md), without changing this permission or confirmation policy.

## Sources

- User refinement discussion (2026-09-30): acceptance of foreground-allow/background-deny/read-deny, fixed 4M without capacity configuration, and the subsequent independent confirmation window recommendation including its lifetime and focus rules.
- [ADR 0007](./0007-retain-separate-paste-confirmation-window.md): existing independent paste confirmation window.
- [ADR 0009](./0009-app-cross-window-input-gate.md): existing application-owned cross-window paste input gate.
- [ADR 0047](./0047-foreground-only-host-authorized-osc52-writes.md): original strict main-window focus policy, superseded to make the confirmation exception explicit.
- [ADR 0048](./0048-bounded-osc52-specific-clipboard-capacity.md): unchanged payload capacity contract.

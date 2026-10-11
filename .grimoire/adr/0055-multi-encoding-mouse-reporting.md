# Multi-Encoding Mouse Reporting

**Status:** Proposed
**Date:** 2026-10-11
**Supersedes:** [ADR 0038 — Prioritized SGR-Only Mouse Reporting](./0038-prioritized-sgr-only-mouse-reporting.md)

## Context

[Issue #192](https://github.com/wangjq4214/harbor/issues/192) extends the #102 SGR-only input boundary with legacy, UTF-8, urxvt, horizontal-wheel and SGR pixel reporting. The refinement conversation on 2026-10-11 approved the recommended contract and a Spec-only endpoint. These choices are not yet implemented or runtime-validated.

The issue's use of “X10” and “press-only” conflates a wire encoding with a tracking mode. Xterm's `?9` is press-only; `?1000` reports press/release, and `?1002`/`?1003` add their respective motion events. Legacy `CSI M` can encode those tracking modes without turning them into `?9`. See [Xterm Control Sequences, Mouse Tracking](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html).

The existing SGR-only policy consumes tracked events without sending bytes when `?1006` is disabled. Keeping that silence would not satisfy legacy applications. Deferring optional `?1005`/`?1015` was considered; the approved scope includes both, sharing tracking and coordinate handling with the other encodings.

## Decision

- Preserve independent tracking state and effective priority `?1003 > ?1002 > ?1000`, with lower-priority fallback on disable and per-mode DECRPM status. Tracking determines which events to report; encoding determines their wire representation.
- Support legacy `CSI M` as the default encoding when tracking is enabled and no extended encoding is enabled. Preserve press/release/motion semantics for the effective tracking mode rather than imposing press-only behavior. Do not add `?9` support in this delivery.
- Independently retain encoding-mode enablement, choosing `?1016 > ?1006 > ?1015 > ?1005 > legacy`. Clearing a higher-priority encoding reveals the next enabled encoding; encoding modes alone do not activate tracking. RIS clears tracking and encoding enablement.
- Support both UTF-8 `?1005` and urxvt `?1015` in this delivery. Suppress a legacy report if either final 1-based coordinate exceeds 223; do not truncate, wrap or clamp an unrepresentable coordinate into a different legacy position.
- Keep coordinate interpretation inside the terminal boundary. Preserve existing cell reporting, and derive `?1016` coordinates from content-relative physical pixels using DPI scale, terminal allocation origin and padding, without first discarding sub-cell precision. Coordinates use a 1-based origin. WheelPixel is a wheel-delta unit, not a coordinate-encoding selector.
- Keep host/widget event transport and local pointer ownership intact. Tracked input continues to take precedence over local selection/scrollback even if an individual event produces no report. Preserve existing SGR press/release/motion/vertical-wheel behavior; add horizontal SGR wheel reports using buttons 66/67. A dual-axis event reports both nonzero axes in a deterministic order, without changing vertical report multiplicity or adding wheel accumulation.

## Consequences

- The old silent-consumption assertion for tracking without `?1006` becomes a legacy-fallback assertion; the existing tracking-priority and SGR regression obligations remain.
- Mode selection, coordinate units and outgoing bytes must agree after pending protocol input has been parsed. Pixel reporting cannot be implemented only in the final formatter after a cell conversion has already discarded position information.
- New supported mode queries must reflect each retained flag, not only the winning encoding. Unsupported modes remain consume-ignored and query as unknown.
- Parser recognition, exact-byte and routing tests are required, but they do not establish Windows/ConPTY application interoperability. [Validation Policy](../../docs/validation.md) remains an independent acceptance gate.
- ADR 0038 preserves the prior #102 decision as superseded history. This Proposed record describes the approved future boundary, not current support. The detailed requirements belong in the Spec-only output, not in an implementation plan or execution tickets.

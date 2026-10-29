# Event-Driven Resize Barrier and Shared Reader Dispatch

**Status:** Testing
**Date:** 2026-10-27
**Sources:** User refinement of the unresolved synchronous resize wait, confirming nonblocking ACK acquisition without long-held Terminal locks, ordered byte handling, latest-target coalescing, explicit recovery, multi-terminal configuration atomicity, and shared ACK dispatch.
**Supersedes in part:** [ADR 0043](./0043-acknowledged-pty-resize-barrier.md), only synchronous UI-thread barrier acquisition. Epoch-qualified acknowledgement, exactly-once reader ordering, pause/resume and bounded failure handling remain required.
**Related:** [ADR 0059](./0059-atomic-manual-reload-and-terminal-only-fonts.md), [ADR 0060](./0060-font-reload-uses-normal-bounded-resize.md), [ADR 0061](./0061-manual-reload-compensation-failure.md).

## Context

The current `TerminalSession::acquire_resize_barrier` in `crates/harbor-terminal/src/io.rs` loops on `recv_timeout` with a two-second barrier deadline. Both ordinary terminal resize and configuration-driven geometry preparation call it. Bounding the wait does not prevent UI stalls. Moving that wait off the UI thread while retaining a Terminal lock for the wait would still prevent other Terminal access.

Ordinary `TerminalIo::drain` currently consumes `ReaderEvent::BarrierAck` without acting on it. That is incompatible with concurrent event-driven acquisition: normal output processing could steal an acknowledgement from the pending resize. Removing the barrier instead would abandon the reader-observed ordering guarantee established by ADR 0043.

Font reload coordinates multiple terminal sessions. Asynchronous acquisition must preserve the existing accepted-configuration boundary and the explicit compensation-failure policy rather than allow individual terminals to publish a candidate independently.

## Decision

Retain the acknowledged resize barrier but wait for its ACK asynchronously through event-driven progress. ACK waiting must neither block the UI nor retain a Terminal lock for the duration of acquisition.

Use shared, ordered reader-event dispatch for normal PTY drain and resize coordination. Bytes preceding the matching ACK are parsed under the old geometry; following bytes on a successful resize are parsed under the committed new geometry. Deliver bytes exactly once. Normal drain must dispatch ACKs to resize coordination rather than consume them as no-ops. Preserve epoch qualification so an obsolete ACK cannot complete a different request. The boundary remains reader-observed, not a timestamp for bytes still unread in an OS pipe.

Coalesce continuous resize requests to the latest target dimensions rather than performing expensive resize work for every intermediate target.

Bound acquisition and recover from timeout, session closure and PTY failure without leaving the reader paused. Preserve the existing transactional geometry guarantees. If complete rollback cannot be achieved, enter an explicit fault-recovery state instead of reporting ordinary success or claiming restoration. For a closed session, this release obligation does not require restarting a closed reader or session.

Font-driven resize across multiple terminals remains one application-coordinated configuration transaction. Asynchronous progress does not authorize partial configuration acceptance. Retain the previous accepted configuration on rejection; when compensation itself fails, preserve ADR 0061's explicit recovery-failed result, reader release, geometry reconciliation attempt and identification of unrecovered sessions.

## Consequences

- Simply shortening the timeout, deleting the barrier, or moving a blocking wait behind a long-held Terminal lock does not satisfy this decision.
- Normal drain, resize progress and closure handling must agree on ownership of ACK delivery; independent consumers cannot race for the same reader events.
- Request merging must avoid repeated expensive work for superseded intermediate targets while retaining stream ordering and transaction safety.
- Required verification includes delayed ACK with UI and Terminal access still progressing, ACK delivery through normal drain, exactly-once old/new-geometry byte handling, superseded targets and epochs, timeout/closure/PTY failure, reader release, explicit incomplete recovery, and multi-terminal rejection without partial accepted configuration.
- This records an approved architectural direction, not completed implementation or runtime evidence. It does not choose concrete public API signatures, a new timeout value, lock-time budgets, or fault-recovery exit mechanics.

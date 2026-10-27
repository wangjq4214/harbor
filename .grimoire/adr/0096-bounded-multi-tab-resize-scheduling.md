# Bounded Multi-Tab Resize Scheduling

**Status:** Testing
**Date:** 2026-10-27
**Sources:** User refinement of sequential `resize_all_if_changed()` processing; explicit selection of option A (bounded cross-tab scheduling), rather than option B (moving individual expensive operations off the UI thread).
**Related:** [ADR 0095](./0095-event-driven-resize-barrier-and-shared-reader-dispatch.md), [spec 0046](../spec/0046-nonblocking-transactional-terminal-resize.md), [spec 0010](../spec/0010-desktop-terminal-tabs-and-view-macro.md).

## Context

Event-driven reader acknowledgement removes synchronous ACK waiting, but does not by itself bound aggregate UI-thread work across terminals. `TabManager::resize_all_if_changed()` visits terminals sequentially, and a ready terminal can synchronously prepare/reflow buffers, resize the native PTY and commit its model. The host's pending-output path can also complete multiple ready resizes in one turn. Changing only the broadcast loop would leave that second accumulation path intact.

The existing tab contract broadcasts the current terminal allocation to inactive PTYs as well as the active terminal. Deferring inactive geometry until tab activation would change that behavior. Font/configuration reload has a separate application-wide acceptance boundary that ordinary scheduling must preserve.

## Decision

For ordinary allocation-driven resize, select bounded cross-tab scheduling: publish the latest target to all live tabs, prioritize the active tab, fairly advance background tabs and yield to the UI between bounded batches of expensive resize work. Aggregate resize execution in a host turn must not grow into an unrestricted sweep of all ready tabs through broadcast, output, idle or render progress paths.

Keep request intent distinct from committed per-tab geometry and aggregate completion. Superseded pending targets must not later overwrite newer intent, including a return to a previously completed broadcast size; partial completion or failure must not be reported as all tabs complete. Inactive tabs continue progressing without activation or additional child output.

Preserve ADR 0095's acknowledged byte ordering, latest-target merging, bounded acquisition, reader release and available Terminal access while waiting. Preserve the coherent font/configuration acceptance and compensation contracts; ordinary scheduling does not authorize partial configuration publication.

This choice addresses cross-tab accumulation, not arbitrary duration of a single operation. It does not require moving individual reflow/native PTY operations to worker threads, preempting a running transaction or guaranteeing that one expensive tab cannot stall the UI. The selected scope is smaller than redesigning individual-operation execution and Terminal state ownership.

## Consequences

- Active-tab priority must coexist with background progress, not starvation or activation-only resizing.
- Completion can be staggered across ordinary tabs; every live tab must converge to the latest target when requests settle and applicable transactions succeed.
- A shared per-turn bound must cover actual transaction entry paths, not only target broadcast. This does not select a scheduler type, numeric batch/time budget, public API or threading mechanism.
- Single-tab reflow and native PTY duration remain explicit limitations. This decision records the selected direction, not implementation or runtime acceptance.

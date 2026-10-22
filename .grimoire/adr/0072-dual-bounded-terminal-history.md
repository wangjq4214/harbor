# Dual-Bounded Terminal History

**Status:** Proposed
**Date:** 2026-10-20
**Sources:** [Issue #166](https://github.com/wangjq4214/harbor/issues/166) and the refinement conversation: the user approved the recommendation of physical-row and retained-history-content byte limits, separate process-memory measurement, and the spec → slice route. This record captures the selected direction, not implementation or acceptance.
**Refined by:** [ADR 0073](./0073-session-history-accounting-and-text-unit-overflow.md), which settles the accounting/reload/oversized-unit boundaries left open in this initial decision; those open statements below describe the earlier refinement stage.

## Context

Harbor's normal buffer currently has a fixed 1000-physical-row history capacity. Primary reflow already applies oldest-first capacity eviction, marks a partially retained logical line as head-truncated, and reconciles content anchors. Text units also retain variable-length Unicode suffixes, so a physical-row count alone does not bound retained source-text bytes.

Issue #166 seeks configurable bounded history and reproducible resource/performance acceptance. A configurable row limit with memory estimates alone would not enforce a retained-content byte ceiling. Conversely, a content-byte limit does not measure preallocated grid storage, metadata, temporary reflow allocations, native font resources, or GPU residency.

## Decision

Use independent physical-row and retained-history-content byte limits rather than a row limit alone. When either history limit is exceeded, evict the oldest physical rows to retain the newest content. Preserve existing logical-line truncation metadata and content-anchor eviction semantics rather than replacing the ring buffer or reverting to physical-coordinate identity.

Do not arbitrarily split ordinary Unicode text units to save history space. An explicit protective policy for an exceptionally large individual text unit is required before the contract is execution-ready; it must apply before unlimited live-cell suffix growth, not only when a row becomes history. The exact exception, accounting domain, and interaction with the existing Unicode retention contract remain unsettled and are not authorized by this record.

Measure process memory separately from the hard retained-content limits. Distinguish fixed grid/metadata storage, variable retained text, temporary preparation overlap, font/native caches, and GPU resources. A source-content byte cap must not be presented as an entire-process or GPU-memory ceiling. Establish default capacities, maximum accepted configuration values, and performance thresholds through comparable measurements, rather than inventing numerical claims.

This extends the bounded eviction direction in [ADR 0042](./0042-content-anchors-and-buffer-specific-resize.md) and [ADR 0060](./0060-font-reload-uses-normal-bounded-resize.md). It does not yet amend the text-preservation policy in [ADR 0044](./0044-unicode-text-unit-and-presentation-width-policy.md); any exceptional input-loss rule requires explicit clarification and linked recording before artifact work relies on it.

## Consequences

- History capacity becomes an enforceable row/content contract, not an unlimited-history promise or a memory estimate alone.
- Content accounting and oldest-first eviction need consistent coverage across output and reflow. Budget updates, reload timing, and saved/alternate buffer accounting still require a settled requirements contract.
- Variable-length text-unit protection cannot silently change ordinary copy, search, rendering, or anchor semantics.
- Process/private-byte and GPU/atlas measurements remain separate acceptance work; allocation-instrumented elapsed time is not latency evidence.
- This decision does not select a parser thread, broad storage rewrite, unmeasured numeric defaults, or any performance optimization.

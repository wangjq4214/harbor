# Demand-Bounded Box and Decoration Storage

**Status:** Completed
**Date:** 2026-10-31
**Supersedes:** [ADR 0056](./0056-terminal-owned-procedural-box-drawing.md), only its fixed per-cell storage and underline slot/offset preservation constraints. All other presentation and ownership decisions remain applicable.
**Sources:** Owner refinement conversation on 2026-10-31: approved limiting the work to terminal procedural boxes and underline/strikethrough/overline, addressing both sparse-content over-allocation and capacity reclamation after window shrink, allowing bounded spare capacity, and revising the old fixed-slot constraints. Selected endpoint: short spec only.

## Context

Skipping empty-slot uploads does not remove fixed-slot allocation. Current box and decoration preparation still reserves worst-case geometry for every grid cell, and GPU buffers and CPU staging retain historical capacity. Shrinking the window alone therefore does not recover that capacity. Merely adding resize-driven shrink would still leave sparse content paying for full-grid worst-case geometry.

The fixed layout was selected to keep local updates bounded. The owner approved changing that layout constraint while retaining local-update capability and existing rendering semantics.

## Decision

- Replace the obligation to reserve full-grid maximum geometry for terminal procedural boxes and underline, strikethrough and overline with demand-bounded storage. Avoid both sparse-content full-grid geometry reservation and indefinite historical-maximum GPU/CPU staging retention after window shrink.
- Allow bounded spare capacity to avoid frequent reallocation. After window shrink or content reduction, check and reclaim on the next successful prepare, without relying on an idle timer. For GPU geometry buffers and CPU vertex staging, capacity exceeding four times current effective geometry demand must shrink to no more than twice that demand, subject to a fixed necessary minimum allocation. Empty content releases geometry storage except for necessary minimal placeholders.
- Measure application-held GPU buffer sizes and CPU staging capacities. Do not promise immediate physical-VRAM reclamation by the driver. Exact storage layout remains an implementation choice.
- Retain local updates, existing presentation semantics, and equivalence between incremental preparation and a fresh complete preparation. Preserve renderer-owned resources and the GPU-independent terminal engine/update boundary.
- This change releases the fixed-slot allocation and underline upload-offset constraints in ADR 0056 and [spec 0022](../spec/0022-box-drawing-and-decoration-lifecycle.md), not their visual, damage, lifecycle or safety guarantees.
- Scope this change to these terminal geometry layers. It is not a blanket reclamation policy for every rendering cache.

## Consequences

- Skipping empty uploads alone, or shrinking full-grid slots only on resize, is insufficient to satisfy the selected direction.
- Storage layout and bookkeeping may change; local-update correctness and removal of obsolete paint must survive those changes.
- [Spec 0050](../spec/0050-demand-bounded-box-and-decoration-storage.md) defines the approved next-successful-prepare timing, four-times trigger and two-times reclamation target. These rules were separately confirmed by the owner in the same refinement conversation.
- This records an approved direction, not implementation, tests or memory/performance measurements.

## Implementation evidence — 2026-10-31

After the owner explicitly invoked `grimoire-loop`, the selected demand-bounded direction was implemented for all four scoped layers using compact row-retained triangles. Empty rows hold no GPU/CPU geometry allocation (M=0). The layer-wide capacity check includes pre-truncation storage and projected growth reserves, preserving the 4x/2x contract without routine full-screen local-edit uploads. [Spec 0050's verification summary](../spec/0050-demand-bounded-box-and-decoration-storage.md#implementation-verification--2026-10-31) records deterministic allocation/work assertions, dense-reference readback, both GPU backends and final Rust quality checks. Overlay verification is retained in spec 0049 rather than a separate developer report; the repaired references pass the full documentation check. Unmeasured desktop/physical-memory behavior remains an explicit limitation, not passing evidence.

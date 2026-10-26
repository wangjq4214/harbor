# Public Renderer Paint-Order Boundary

**Status:** Proposed
**Date:** 2026-10-26
**Sources:** [UI_API_REVIEW.md](../../UI_API_REVIEW.md) U6/F14 and §§4–6; the refinement conversation approving the proposed public API boundaries, atomic migration without compatibility aliases, and a single-spec endpoint.
**Related:** [ADR 0003](./0003-widget-independent-pipeline.md).

## Context

QuadRenderer retains GPU instances in reusable storage slots. Its current public encode method draws a storage range, not SceneItem.paint_order; encode_range and slot_of expose that storage model to independent consumers. Insertion, reordering and slot reuse can make storage order differ from scene paint order. Stable slots alone therefore cannot establish correct occlusion.

Runtime's frame encoder already walks the ordered scene, resolves slots and flushes batches around other rendering work. F14 is a public low-level API contract gap, not evidence that Runtime's existing scene ordering is wrong. The existing [runtime guide](../../docs/developer/widget-runtime.md) requires that only adjacent compatible items batch and that transparent primitives not cross paint boundaries.

The review allowed internalizing storage operations or explicitly exposing storage-order drawing with an ordered-items path. The user selected an independently usable public QuadRenderer with ordered drawing, while keeping storage operations internal rather than retaining a second public storage-order API.

## Decision

1. **Recommend Runtime for ordinary scene rendering.** Preserve its existing paint-ordered entry points, including the distinction between caller-owned render-pass encoding and full-frame encoding for backdrop filters. Do not route ordinary consumers through raw GPU storage slots.
2. **Keep independent public QuadRenderer use with an explicit ordered entry point.** Its drawing contract follows scene paint order for the primitives it supports, independently of allocation, retained slot identity, insertion history or slot reuse. Consumers must not need slot lookup or range-drawing calls to obtain correct ordering. This does not turn QuadRenderer into a replacement for Runtime's mixed-primitive, external-draw or backdrop orchestration.
3. **Internalize storage capabilities and migrate once.** Slot lookup and range drawing become internal. Remove the ambiguous unordered public drawing entry; migrate affected in-repository callers, tests, exports and documentation together without compatibility aliases. Concrete ordered-entry spelling and signature are not fixed by this record.
4. **Preserve retained reuse and batching.** Storage slots remain an internal retention mechanism, not drawing order. Preserve reuse and adjacent compatible batching without reordering transparent content across paint boundaries or rewriting the renderer wholesale.
5. **Require independent-entry GPU evidence.** Verify correct occlusion after reordering and removal/reinsertion, including translucent overlap, through the supported public independent entry. Keep internal reuse/batching evidence separate: neither stable-slot assertions nor Runtime-only tests establish that public contract.

## Consequences

- Independent consumers receive an explicit paint-order contract without inheriting the renderer's allocation model.
- Removing public storage operations and ambiguous unordered drawing is an intentional API break; affected in-repository consumers and documentation must migrate together.
- Tests of internal storage mechanics can remain internal; external integration tests must demonstrate the supported public contract rather than reconstruct order through slot APIs.
- Runtime scene-order orchestration, retained GPU storage and batching remain valuable existing mechanisms; this decision does not authorize rewriting them or changing U1–U5 contracts.
- GPU readback establishes draw-result evidence; unavailable adapters or skipped GPU tests do not establish acceptance.
- This Proposed record captures the approved direction, not completed implementation or executed verification. ADR 0003's independent widget-pipeline ownership remains unchanged.

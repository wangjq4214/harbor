# U6 — Public Renderer Layers and Paint Order

**Spec ID:** 0043
**Status:** Draft
**Date:** 2026-10-26
**Sources:** [UI_API_REVIEW.md](../../UI_API_REVIEW.md) U6/F14 and §§4–6; the refinement conversation approving Runtime as the recommended ordinary entry, an independently usable ordered QuadRenderer, internal storage operations, atomic migration without compatibility aliases, and preservation of reuse/batching.
**Decision record:** [ADR 0089](../adr/0089-public-renderer-paint-order-boundary.md).

This is a new requirements contract, not an implementation or acceptance report. The earlier owner-acceptance baseline for existing specifications does not establish delivery of this new U6 contract.

## Requirements

### R1 — Distinguish ordinary scene rendering from independent quad rendering

- Recommend Runtime's paint-ordered rendering entry points for ordinary consumers. Preserve the existing caller-owned render-pass path and full-frame path, including the latter's backdrop-filter responsibility.
- Keep QuadRenderer publicly usable independently of Runtime for its supported primitives. An independent consumer must have a complete supported path from scene updates and drawing-order input to correctly ordered GPU output without accessing storage slots.
- Document each layer's responsibility. QuadRenderer is not a replacement for Runtime's text/external/backdrop orchestration, and a stable GPU slot is not a paint-order index.

Source: F14; ADR 0089 D1–D2; [runtime guide](../../docs/developer/widget-runtime.md).

### R2 — The public independent path follows scene paint order

- Provide an explicitly ordered public drawing entry. For a valid scene of supported primitives, output follows scene paint order rather than allocation order, SceneDelta insertion history, numeric IDs or reused slot indices.
- Honor order changes between frames even when retained item IDs and GPU slots remain unchanged. Adding an item between retained items, removing an item, and later inserting/reinserting content must not make storage order determine visible occlusion.
- For overlapping translucent content, preserve the scene's compositing order. Do not obtain a convenient storage range by moving a draw across another paint item.
- Public documentation must explain the ordering input and the caller/renderer responsibilities needed to use it correctly. Concrete method spelling, signatures, sorting/preparation mechanism and data structures are not frozen here; none may require public slot queries or range calls to satisfy the ordered contract.

Source: F14; review U6; ADR 0089 D2/D4.

### R3 — Storage operations are internal, not an alternative public rendering contract

- Make slot lookup and storage-range drawing internal capabilities. The existing public slot_of and encode_range capabilities must not remain available to independent consumers under those names or equivalent public aliases.
- Remove the ambiguous unordered public drawing entry currently exposed by QuadRenderer::encode. A supported public entry must not silently retain its storage-order semantics; the concrete replacement signature/name is left to implementation.
- Migrate affected in-repository consumers, tests, module exports and documentation together. Do not retain compatibility aliases for the removed public contract.
- Public examples and independent integration tests use the supported ordered path, not a hand-built slot walk. Internal storage/reuse tests may exercise internal mechanisms without making them public again.

Source: approved refinement API/migration boundary; ADR 0089 D3.

### R4 — Preserve internal retention, batching and Runtime ordering

- Preserve retained GPU slot reuse and existing batching capability. Separating storage order from drawing order does not authorize rebuilding the renderer wholesale or abandoning retention.
- Batch only adjacent compatible work in scene order. Slot adjacency alone does not permit merging draws across a different intervening paint item.
- Preserve Runtime's existing ordered scene walk and its batch boundaries around other rendering work. U6 addresses independent public usage; it is not evidence that Runtime's current ordering is broken.
- Preserve U1–U5 contracts and the review's ownership/lifecycle boundaries. Do not change native Host ownership, GPU-fault policy or HMR teardown as part of this unit.

Source: review §§4–5; ADR 0089 D4; runtime guide; [ADR 0003](../adr/0003-widget-independent-pipeline.md).

### R5 — Establish public-boundary and real draw-result evidence

- Verify that an external consumer can compile and render through the independent ordered entry without private renderer access. Verify that removed storage capabilities are no longer part of the public API.
- Verify GPU output after reordering, insertion, removal and reinsertion, including translucent overlap. Choose scenes whose correct compositing result differs from storage-order output.
- Keep internal reuse/batching evidence and public GPU output evidence distinct. Stable-slot assertions, successful command encoding, and Runtime-only rendering tests do not establish the independent entry's occlusion contract.
- Report GPU adapter/backend availability and actual execution. A skipped/no-adapter test is not successful GPU acceptance, and existing tests are evidence entry points rather than results of this refinement.

Source: review U6 and §6; ADR 0089 D5; existing GPU-test adapter/readback seams.

## Solution and Integration Seams

Continue the existing retained renderer architecture. Expose an explicitly ordered independent drawing path, keep storage allocation/range capabilities private to the crate, and retain Runtime as the ordinary full-scene entry. Do not expose internal storage again merely to preserve old external tests.

| Seam | Connects | Expects | Provides |
| --- | --- | --- | --- |
| Independent rendering | external consumer → public QuadRenderer | scene updates, viewport and the documented scene-order input for supported primitives | GPU output in paint order without slot/range access |
| Retained scene | SceneItem/SceneDelta and scene order → retained GPU renderer | identity/update information is distinct from ordering | edits/reorder/removal/reinsertion do not substitute storage order for paint order |
| Runtime coordination | ordered scene walk → internal quad batching and other draw work | preserve existing scene/external/backdrop boundaries | ordinary Runtime rendering remains ordered and retained |
| Public migration | module exports, callers, tests and examples → supported entry | one coherent public API migration, no compatibility aliases | independent usage compiles while storage-only capabilities are unavailable |
| Verification | independent public calls → submitted GPU work → texture readback | order-sensitive opaque/translucent overlap scenes | observable occlusion/compositing evidence separate from slot-retention assertions |

### Verified source baseline

At commit `41a5016`, source inspection found:

- [QuadRenderer](../../crates/harbor-widget/src/renderer/quad.rs) exposes update, encode, encode_range and slot_of; encode draws a storage range.
- [FrameEncoder](../../crates/harbor-widget/src/runtime/frame_encoder.rs) already coordinates ordered scene encoding and internal slot-based batches. [Runtime](../../crates/harbor-widget/src/runtime/mod.rs) exposes ordinary and full-frame encoding.
- [SceneGraph](../../crates/harbor-widget/src/scene/mod.rs) exposes retained items in paint order, separately from SceneDelta updates.
- [Renderer integration tests](../../crates/harbor-widget/tests/quad_renderer.rs) contain GPU adapter creation and texture readback, but their helpers currently call encode directly or reconstruct ordering with slot_of/encode_range. Those helpers must migrate rather than preserve public storage APIs.

These observations locate affected contracts; they are not tests executed for U6.

## End-to-End Acceptance

### E1 — Independent public rendering does not inherit insertion order (R1–R3, R5)
- **Given:** overlapping supported primitives whose scene paint order differs from the order in which their retained instances were added.
- **When:** an external consumer applies scene updates and renders using only the supported public ordered path, submits the GPU work and reads the target.
- **Then:** overlap pixels match scene paint order, not storage/insertion order. No slot lookup, storage-range call or Runtime wrapper is needed.

### E2 — Reordering retained items changes visible occlusion (R2, R4–R5)
- **Given:** retained overlapping items already rendered in one order.
- **When:** their scene paint order changes without changing their identities, and a later frame is rendered through the independent public entry.
- **Then:** readback reflects the new order even though retention permits their slots to stay unchanged. Internal tests separately establish reuse and retained batching capability; pixel correctness alone is not reuse evidence.

### E3 — Insertion, removal and reinsertion cannot turn reuse into draw order (R2, R4–R5)
- **Given:** overlapping retained items and a storage history that frees slots.
- **When:** content is inserted between retained paint items, removed, and inserted/reinserted in a later paint position.
- **Then:** successive independent-entry GPU results match the current scene order; removed content does not remain visible, and reuse does not draw newly inserted content at the old slot's paint position.

### E4 — Translucent overlap preserves compositing order (R2, R4–R5)
- **Given:** distinguishable translucent overlapping primitives for which swapping order changes the composed color.
- **When:** they are drawn, reordered and drawn again through the independent public entry.
- **Then:** readback matches the intended compositing order for each frame. Batching must not conceal an order violation by regrouping storage-adjacent but paint-separated items.

### E5 — Public migration and Runtime regressions remain coherent (R1, R3–R5)
- **Given:** an external consumer, affected repository callers/examples, internal reuse tests and existing Runtime scene-rendering coverage.
- **When:** the new public API and migrated consumers are checked.
- **Then:** supported independent usage compiles; public slot lookup/range access and the old unordered entry are unavailable; no compatibility aliases keep them reachable. Internal retention/batching coverage remains meaningful, while Runtime still preserves scene order and boundaries around non-quad work.

## Decisions, Assumptions and Completion

- The approved boundary is recorded in ADR 0089; no new domain definition or supersession of independent pipeline ownership is required.
- This is one bounded public API outcome with a continuing contract, so one spec is selected without a ticket set or implementation-plan stage.
- No unconfirmed product or architecture assumption is used. Low-level API names and ordering mechanisms remain implementation details subject to R1–R5, not assumed existing APIs.
- Completion needs independent-entry compilation and GPU draw-result evidence, internal retention/batching evidence, migrated consumers/docs and preserved Runtime regression contracts. No new performance threshold or mandatory backend matrix is invented.
- This refinement does not run implementation, QA or GPU tests. Any unavailable execution evidence must remain explicitly reported, not inferred from earlier acceptance of other specs.

## Out of Scope

- A generic renderer/API v2, a wholesale renderer rewrite, or exposing a second public storage-order API.
- Turning QuadRenderer into Runtime's full mixed-scene, text, external-draw or backdrop compositor.
- Redoing U1–U5, changing native hosting/fatal-error policy, or weakening existing ownership and lifecycle contracts.

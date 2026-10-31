# Mutation Revisions and Incremental Terminal Updates

**Spec ID:** 0045
**Status:** Draft
**Date:** 2026-10-27
**Sources:** Refinement conversation on 2026-10-27: the owner selected both revision-based acknowledgement and removal of unconditional full-grid cloning, then approved this spec endpoint, A → B → A invalidation, and the full-reconstruction exceptions below. [ADR 0094](../adr/0094-mutation-revisions-and-incremental-terminal-updates.md) records the architectural direction.

## Requirements

### R1 — Revision-based acknowledgement, not grid equality

The renderer update contract must validate acknowledgement using mutation revisions rather than comparing snapshot cells. A current, safely prepared update can be acknowledged; an update made stale by a relevant intervening mutation must be rejected without consuming the pending changes.

In particular, reading state A, mutating A → B → A, then acknowledging the original update must fail even though the final content is equal. A subsequent current update remains available for preparation and acknowledgement. Validity checking must not traverse or hash the whole visible grid to discover whether it changed. Moving that comparison into update construction does not satisfy this requirement.

### R2 — Incremental or reusable update content

The renderer-facing update-read path must not unconditionally clone all visible cells on every read. Once initialized, unchanged reads reuse unchanged grid content; local mutations update only the affected projection portions, without unconditionally copying the entire visible grid. This requirement applies across the mutation-to-read path: moving the same full-grid copy into every mutation, or triggering it because an older update remains retained, is not a solution.

Initial projection, resize and invalidation reconstruction may perform full-grid work. Work genuinely affecting the full viewport may also require all affected content; incremental does not mean omitting changed cells. The implementation must distinguish these cases from ordinary unchanged/local-change updates rather than label every update as reconstruction.

Granularity and representation are not prescribed: this contract does not mandate per-cell patches, row sharing, a particular smart pointer, or a public API shape. Document the chosen granularity and actual copied work so the requirement can be verified.

### R3 — Coherent content and validity coverage

Revisions validate an update; they do not replace the coherent content needed by the renderer. An update must describe one consistent engine state, and a retained earlier projection must not silently change when the engine mutates or a later update is read.

Coverage includes all state whose changes can make acknowledgement unsafe, not just cells. Preserve the existing snapshot/update inputs: dimensions, cursor position/style/visibility/blink, scroll/view/history coordinates, wrap metadata, active buffer, input modes, damage, selection, preedit, appearance/palette and backdrop state, together with pending redraw/frame-demand behavior. Account for their changes without restoring full-grid comparison. Reading time-dependent frame demand does not itself prove that content was consumed.

The current projection acknowledgement/invalidation epoch is not by itself a mutation revision. Preserve its invalidation/replay purpose. Likewise, existing semantic content revisions used by [spec 0027](./0027-revisioned-terminal-read-model-and-literal-search.md) do not by themselves establish that all renderer-update inputs are unchanged. Any reuse must retain both contracts' semantics; no particular revision layout is selected here.

### R4 — Preserve explicit consumption and recovery

Retain [ADR 0045](../adr/0045-gpu-independent-terminal-core-boundary.md) and [spec 0016](./0016-terminal-engine-wgpu-renderer-boundary.md):

- The engine remains GPU-independent and supplies coherent read-only updates to the existing concrete renderer without mutable parser/screen access.
- Reading/re-reading an update does not acknowledge damage or clear pending redraw.
- Acknowledgement occurs only after renderer preparation is sufficient to draw that update. Stale acknowledgement, skipped drawing and failed preparation must leave pending changes available for replay or full reconstruction.
- Initial/invalidated projections retain full reconstruction semantics. Old acknowledgements cannot become valid through projection invalidation or acknowledgement of a different update.
- Preserve the existing single-renderer-per-session boundary; this change does not introduce multi-renderer subscriptions.

### R5 — Preserve rendering and terminal behavior

The revised update path must produce the same current terminal presentation as a fresh complete projection, including local edits, erase and structural changes, wide cells/Unicode suffixes, scrolling/review, primary/alternate changes and reset. Preserve the existing resize, DPI/viewport, palette, selection, cursor, preedit, scrollbar, retained/live drawing and scheduling behavior required by spec 0016. Existing independent content readers must still leave renderer damage available under spec 0027.

## Solution and integration contracts

Use mutation-revision validation for acknowledgement and incremental/reusable coherent content for renderer reads. Keep full reconstruction available for the approved cases. Storage layout, revision placement and projection granularity are implementation choices, not additional requirements established by this spec.

| Seam | Connects | Expects | Provides |
| --- | --- | --- | --- |
| State changes → update validity | Engine state owners → update/acknowledgement boundary | Relevant cell and non-cell mutations are represented; A → B → A remains distinguishable from no intervening mutation | Old update rejection without grid equality |
| Engine → renderer | Terminal update read → existing concrete pipeline | One coherent read-only content projection, damage and presentation inputs | Reusable/incremental ordinary reads and sufficient content for preparation |
| Prepared renderer → engine | Preparation result → explicit acknowledgement | Only a successfully prepared, still-current update can consume pending changes | Safe acknowledgement or rejection with replay preserved |
| Projection loss → recovery | Invalidation/geometry change → update and renderer reconstruction | Retained projection may be uncertain or incompatible | Complete reconstruction without prematurely consuming damage |

Source integration surfaces are [`lib.rs`](../../crates/harbor-terminal/src/lib.rs), [`screen.rs`](../../crates/harbor-terminal/src/screen.rs), [`update.rs`](../../crates/harbor-terminal/src/update.rs), [`model.rs`](../../crates/harbor-terminal/src/model.rs) and [`render/pipeline.rs`](../../crates/harbor-terminal/src/render/pipeline.rs). These identify current exchanges, not mandatory edit steps or an API freeze.

## End-to-End verification

| Scenario | Input/trigger | Required outcome |
| --- | --- | --- |
| Unchanged repeated reads | Initialize/acknowledge, then repeatedly read and prepare without content mutations | Coherent identical grid content without repeated full-grid cloning or validity equality scans; reads alone do not consume pending state |
| Local edits with an older update retained | Retain an update; write/overwrite/erase a bounded region, then read again | Old projection remains stable; new projection matches a complete reference; copied content follows the documented affected granularity rather than the whole viewport |
| Mutation between read and ack | Read A; apply a relevant mutation; attempt to acknowledge A; then prepare/acknowledge the current update | Old ack fails and leaves changes pending; current update can converge and be acknowledged |
| A → B → A | Read A; change a cell or relevant non-cell value and restore it; acknowledge the original update | Original ack fails despite equal final values; a fresh update can be prepared and acknowledged |
| Non-cell-only changes | Change cursor/modes, wrap/view metadata, selection, preedit, palette or backdrop without ordinary cell writes, as applicable | Presentation and acknowledgement validity reflect the change; no need to clone unchanged cells just to supply the new metadata |
| Accumulation and failed preparation | Read updates while drawing is hidden/skipped; mutate repeatedly; simulate preparation failure, then resume successfully | All still-pending changes are replayed/reconstructed; intermediate reads and failure do not clear damage/redraw |
| Full reconstruction and stale tokens | Initial creation, resize or explicit invalidation, including a previously retained update | Complete correct projection is available; a pre-invalidation/pre-resize stale ack cannot clear the new work |
| Structural and buffer changes | Scroll/review, insert/delete/move, primary/alternate transitions and reset with retained updates | Current content/coordinates/wrap/cursor match a fresh complete projection; no wrong-buffer content or stale acknowledgement |
| Existing consumers | Render under DPI/viewport/font/palette changes, selection/preedit and retained/live scheduling; run independent content reads | Existing spec 0016/0027 behavior remains intact and updates remain renderable |

## Test and performance evidence

Use focused deterministic tests for revision advancement/stale rejection and damage preservation, plus renderer integration/reference comparisons for projection correctness. Include unchanged reads, bounded edits, older updates kept alive, non-cell-only changes, structural changes, failure/retry and reconstruction. A full projection may serve as a test oracle without returning full-grid comparison to the production acknowledgement path.

Demonstrate both requested removals separately: (1) no production validity traversal of all cells, and (2) no unconditional full-grid copying in ordinary mutation/read paths. Use copy/clone/allocation or equivalent work observations and code-path inspection, not wall-clock timing alone. Compare fixed bounded edits and unchanged reads across different viewport sizes; report actual copying granularity and distinguish reconstruction from steady-state work. Include an older retained update so a whole-grid copy-on-write fallback is not hidden by the easiest ownership case.

Record reproducible before/after measurements separating mutation, update construction and acknowledgement, with workload, build profile/features, grid sizes, session count, environment and revision/dirty scope. No new numerical speedup, latency or memory threshold is selected. This contract removes the two identified update-boundary costs; it does not assert that every renderer layer or the entire frame becomes constant-time.

Preserve the applicable core-only/rendered build and regression gates from spec 0016. Record focused Windows rendering/recovery observations for the affected behavior and label results PASS / FAIL / NOT RUN / BLOCKED honestly. Do not treat this spec, source inspection, or an unchanged old acceptance result as fresh implementation evidence.

## Decisions and boundaries

| Choice | Source/rationale |
| --- | --- |
| Solve both costs together | Owner selected both after distinguishing acknowledgement comparison from update-read cloning |
| Revision changes reject old acknowledgement even after A → B → A | Explicit owner confirmation; no equality scan to revive an old token |
| Permit initial/resize/invalidation full reconstruction | Explicit owner confirmation; recovery remains available without making every ordinary update full |
| Keep coherent renderer data and explicit damage consumption | ADR 0045, spec 0016 and ADR 0094 |
| Leave concrete storage and API representation open | Owner-confirmed scope and ADR 0094; no new representation was selected |

This endpoint is requirements only: no tickets, implementation plan, production changes or QA execution are authorized by writing the spec. No new renderer/backend or multi-renderer protocol is introduced. There are no additional assumed product requirements or unresolved blocking choices in this contract; implementation and performance acceptance remain to be demonstrated.

## Delivery and verification — 2026-10-27

The implementation uses row-shared `SnapshotCells`, visible row-version caching and session-qualified mutation/epoch/snapshot-identity acknowledgement proofs. No per-cell equality scan remains in production acknowledgement. Old retained updates do not force a whole-grid payload copy; the chosen granularity is a row. Mutable selection provenance lives inside the selection owner so an empty PTY scheduling poll does not invalidate an update merely by passing a mutable pointer argument.

[Verification record](../../docs/developer/incremental-terminal-updates.md) maps R1–R5 to source/tests, documents public ownership compatibility, reports deterministic copy counts, actual before/after CPU measurements, workspace/core/rendered/native-GPU results and limitations. In the 160×400 bounded-edit measurement, update reads changed from 680.476 µs to 3.362 µs and acknowledgement from 320.100 µs to 0.471 µs (one session; no whole-frame claim).

Implementation is delivered, but complete acceptance is not claimed: strict Clippy fails at unchanged existing Rust 1.99 lint sites, and full interactive application/ConPTY acceptance was not run. The spec status is not promoted on partial evidence. No tickets or commits were created.

Supplemental allocation and Release timing evidence is consolidated in [spec 0051](./0051-reusable-terminal-hot-path-storage.md). Latest-snapshot sharing and uniquely owned wrap/damage storage reuse preserve this spec's mutation-validity, immutable-old-projection and explicit-consumption contracts. Unchanged reads improve substantially in the recorded fixture; fewer local-edit allocations do not imply comparable elapsed-time gains. The Clippy failure noted above describes the earlier implementation checkpoint; spec 0051 records later passing strict Clippy runs without implying fresh interactive acceptance.

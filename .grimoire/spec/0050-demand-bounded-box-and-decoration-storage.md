# Demand-Bounded Box and Decoration Storage

**Spec ID:** 0050
**Status:** Implemented
**Date:** 2026-10-31
**Sources:** Owner refinement conversation on 2026-10-31: approved the scoped demand-bounded allocation direction and short-spec-only endpoint, then separately approved next-successful-prepare reclamation, a >4x trigger, a <=2x target, minimal empty storage and application-held capacity measurement. [ADR 0101](../adr/0101-demand-bounded-box-and-decoration-storage.md) records the decision and partial supersession of [ADR 0056](../adr/0056-terminal-owned-procedural-box-drawing.md).

## Requirements

### R1 — Allocate for active geometry, not every grid slot

Scope: terminal procedural box drawing, underline, strikethrough and overline. Their GPU geometry buffers and CPU vertex staging must not reserve the full grid's maximum geometry merely because the grid exists. Empty and sparse layers must have correspondingly minimal or demand-bounded geometry storage. Skipping empty uploads without removing the allocation is insufficient; shrinking full-grid buffers only on resize is also insufficient.

Capacity accounting must distinguish current active geometry from unused capacity and inactive-cell slots. The latter cannot be counted as effective demand to satisfy the bound. This does not require a particular allocation layout or change to individual shapes' tessellation.

### R2 — Reclaim historical capacity on the next successful prepare

After window shrink or content reduction, check capacity and perform required reclamation within the next successful prepare; do not wait for an idle timer, session recreation or application restart. Apply this to both GPU buffers and CPU vertex staging, not just their drawn counts or vector lengths.

For each scoped layer, account for GPU geometry capacity and CPU vertex-staging capacity separately in bytes. Let `D` be current effective geometry demand and `M` the fixed necessary minimum allocation for the storage being measured:

- Retained capacity is bounded by `max(M, 4 * D)`.
- If existing capacity exceeds that bound, reclaim to at most `max(M, 2 * D)` while still supporting the required geometry.
- At zero demand, release geometry storage except for necessary minimal placeholders; capacity is at most `M`.
- Document the necessary minimum and account for all buffers/staging contributing to the measured storage. `M` must not grow with grid dimensions or historical demand.

The four-times threshold allows spare capacity without requiring reallocation on every edit. Crossing it requires actual capacity reduction, not just clearing vertices. Check the exact boundary as well as values above it. No particular growth algorithm or row/block/pool layout is selected.

### R3 — Preserve local updates and rendering

Preserve bounded local-update capability rather than replacing every local edit with a full-screen geometry rebuild/upload. Changed storage must clear removed geometry and retain correct placement through row-stride changes, shrink/regrow, palette, viewport and font/metric changes. Reclamation or necessary relocation may rebuild affected storage; it does not authorize routine full-screen work for local edits with stable resources/capacity.

Incremental output must equal a fresh complete preparation of the same current projection. Preserve procedural joins, underline styles/colors, decorated-space and wide-cell behavior, strikethrough/overline semantics, foreground/inverse/conceal handling, clipping and paint order from [spec 0022](./0022-box-drawing-and-decoration-lifecycle.md). Only its fixed-slot storage and underline slot/offset preservation constraints are replaced.

Keep resources renderer-owned and the terminal engine/update boundary GPU-independent. Preserve safe invalid-input handling and pending damage across skipped/failed preparation; the reclamation deadline is the next successful preparation, not an invalid or skipped frame. No stale geometry may reappear after regrowth or recovery.

### R4 — Verify held capacity, not immediate driver release

Observe application-held GPU buffer sizes and CPU staging capacities, separately from active vertex counts and uploaded bytes. Demonstrate sparse/empty allocation and historical-capacity reclamation with deterministic observations. Do not infer reclamation from an empty draw, fewer uploads, or a smaller vector length alone.

The guarantee concerns the application's held allocations. It does not promise immediate physical-VRAM return by the driver or a numerical process-memory/latency improvement. Report non-geometry bookkeeping separately so these geometry bounds are not presented as a bound on all renderer memory; inactive-slot vertex payload cannot be relabeled as bookkeeping.

## Solution and integration contracts

Use demand-bounded retained geometry with the approved reclamation rule; exact private APIs, mapping and allocation units remain implementation choices. The existing source surfaces are [`render/box_drawing.rs`](../../crates/harbor-terminal/src/render/box_drawing.rs), [`render/decoration.rs`](../../crates/harbor-terminal/src/render/decoration.rs), [`render/gpu.rs`](../../crates/harbor-terminal/src/render/gpu.rs) and [`render/pipeline.rs`](../../crates/harbor-terminal/src/render/pipeline.rs). Their current fixed-slot names and offsets are not compatibility requirements.

| Seam | Exchange | Guarantee |
| --- | --- | --- |
| Coherent update → scoped layers | Current cells/grid and accumulated damage | Determine current geometry; retain local updates without consuming skipped/failed damage |
| Projection/resources → geometry | Viewport, palette and font metrics/resources | Refresh actual dependencies while preserving current presentation |
| Geometry → CPU staging/GPU storage | Active demand, capacity and changed geometry | Enforce R1/R2, update required ranges and remove obsolete paint |
| Retained storage → draw/observations | Current geometry ranges and held allocation sizes | Draw only current geometry; expose capacity evidence independently of upload/count evidence |

## Input-to-outcome verification

| Scenario | Given / when | Required result |
| --- | --- | --- |
| Empty/sparse grids | Prepare empty layers, then a few boxes/decorations at multiple grid sizes | No full-grid worst-case geometry reservation; empty minimal storage and sparse capacity obey R1/R2 |
| Window shrink | Warm a large populated grid, shrink substantially and prepare successfully | Both GPU and CPU capacities are checked; excess above `max(M, 4 * D)` reaches at most `max(M, 2 * D)` in that prepare |
| Content reduction without resize | Remove most geometry while dimensions stay unchanged | Same reclamation deadline and bounds; erased geometry disappears |
| Thresholds and empty transition | Exercise capacity at 4x, above 4x, and then zero demand | Boundary does not itself require shrink; above-bound capacity reaches the target; zero retains only necessary minimal placeholders |
| Local churn | Add, change, conceal/reveal and remove geometry in a small region with stable resources and capacity | Bounded local work; unchanged regions retained; image matches fresh complete preparation |
| Shrink/regrow and stride change | Reclaim, regrow, and change columns with no increase in total cells | Correct mappings and full-reference-equivalent image; no stale paint, invalid offsets or out-of-bounds uploads |
| Invalid/deferred preparation | Accumulate changes while preparation is invalid, skipped or failed, then recover | Last-valid-resource and pending-damage guarantees preserved; first successful prepare produces current output and required reclamation |
| Dependency changes | Change palette, viewport or font metrics/resources without new content damage | Correct refreshed geometry and complete-reference equivalence; capacity remains demand-bounded |

Combine capacity assertions with geometry/upload-work observations and practical GPU image/readback comparisons. Include all four layers, dense-to-sparse and populated-to-empty transitions, repeated shrink/regrow and unchanged warm preparations. These are required verification cases, not results from this refinement.

## Decisions, limits and handoff

- **Established choice:** owner-approved demand-bounded storage and 4x/2x reclamation policy; ADR 0101. This partially supersedes the historical storage constraints in spec 0022, not its visual/lifecycle guarantees or historical evidence.
- **No unconfirmed product assumptions or blocking decisions.** Allocation layout, private APIs, necessary placeholder sizes and bookkeeping are implementation details constrained by the observable bounds above, not newly selected architecture.
- **Endpoint:** short spec only. No tickets, implementation plan, code changes, test execution or implementation/acceptance claim.
- **Out of scope:** a blanket policy for other renderer buffers, text/emoji atlases or widget caches; new terminal presentation/protocol semantics; immediate driver-level physical-memory reclamation.

## Implementation verification — 2026-10-31

The owner subsequently invoked `grimoire-loop`, authorizing implementation and verification beyond the refinement-only endpoint above. All four layers now use compact row-retained triangles, with no empty-row GPU/CPU vertex allocation (M=0) and layer-wide 4x/2x reclamation. Local edits rebuild/upload touched rows; unchanged rows retain their storage and warm preparations upload zero bytes. Reclamation includes pre-truncation capacity and projected GPU growth reserves, so growth spare capacity cannot bypass the layer bound.

These are preserved results from the implementation loop, not new test executions during documentation consolidation. Source snapshot: base `cbc4518ac3883799f93b4de0907cc6eeff0e8142` plus the scoped box/decoration/GPU implementation and tests. Production changes are in the existing source surfaces above and [`gpu/colored_rows.rs`](../../crates/harbor-terminal/src/render/gpu/colored_rows.rs); no unrelated cache policy, terminal engine state or public API changed.

### Requirement-to-evidence summary

| Contract | Executed evidence | Result |
| --- | --- | --- |
| R1 — Active geometry storage | Compact only exactly zero-area builder padding; empty rows hold no GPU buffer or allocated vertex Vec. Sparse geometry bytes are identical at 4x8 and 32x64 grids. Dense-builder reference pixels remain identical. | PASS |
| R2 — Timely capacity reclamation | Actual held buffer sizes and CPU capacities obey the strict >4x trigger and <=2x target. Cover exact 4x, above 4x, aggregate trimming including untouched rows, pre-truncation capacity, growth-reserve overshoot, and repeated dense/sparse/empty and shrink/regrow transitions. | PASS |
| R3 — Locality and presentation | Overlapping damage builds each touched row once; unchanged buffers are retained. Repertoire/joins, styles/colors/conceal, stride, invalid bounds/projection, pending damage, palette, DPI and changed-font-resource regressions pass. New compact output matches independently initialized dense-builder readback. | PASS |
| R4 — Held capacity and limits | Observe `buffer.size()` and `Vec.capacity()`, independently of used vertices and write bytes; empty layers hold zero geometry capacity. Report row-descriptor metadata separately, not as effective geometry demand. | PASS |

Focused capacity/compaction coverage is in [colored-row tests](../../crates/harbor-terminal/src/render/gpu/colored_rows/tests.rs); layer integration and dense-reference comparisons are in [box tests](../../crates/harbor-terminal/src/render/box_drawing_tests/gpu.rs) and [decoration tests](../../crates/harbor-terminal/src/render/decoration_tests/gpu.rs). Five new GPU cases and one CPU compaction case were added. Inline review/check covered the production diff, new storage/tests, pipeline consumers and old-contract supersession; no confirmed scoped blocker or unresolved requirement gap remains. A growth-reserve regression was demonstrated failing before the projected-capacity fix and passed afterward.

The storage policy is row-granular, not a new per-cell upload guarantee. Global invalidation or necessary reclamation/relocation may rebuild or reupload affected storage; nonempty rows can add draw calls. Conservative grid/resource validation remains a safety check, not a full-grid allocation request or an expanded supported-grid contract. Public dense geometry builders remain compatible reference fixtures; runtime staging no longer builds their full-grid vectors.

### Observed held capacity

On both adapters, three cycles of an 8x16 populated-to-one-cell-sparse transition produced identical byte counts:

| Layer | Dense GPU capacity | Sparse GPU capacity | Dense CPU vertex capacity | Sparse CPU vertex capacity | Sparse geometry demand |
| --- | ---: | ---: | ---: | ---: | ---: |
| Boxes (`╬` -> `─`) | 294,912 | 576 | 196,608 | 288 | 288 |
| Curly underline | 589,824 | 4,608 | 393,216 | 2,304 | 2,304 |
| Strikethrough | 36,864 | 288 | 24,576 | 144 | 144 |
| Overline | 36,864 | 288 | 24,576 | 144 | 144 |

Populated-to-empty transitions reduce every layer's demand/GPU/CPU vertex capacity to `(0,0,0)`. A separate empty 200-row fixture holds zero geometry bytes and 12,800 bytes of row-descriptor metadata for one layer. Bounded per-cell stack scratch is not retained heap staging. These are application-held allocations, not process RSS, total renderer memory or physical driver VRAM.

### Environment and executed gates

Windows build 26300, x86_64 MSVC, Rust 1.99.1 (58cf8f9c3), LLVM 23.1.1. Native adapter: AMD Radeon RX 7900 XT / Vulkan, driver 26.10.1 (LLPC). Software adapter: Microsoft Basic Render Driver / DX12 WARP, driver 10.0.26100.9711. Tests use the repository offscreen fixture and system fonts, including Consolas 16 -> 23 changed-metrics replacement.

Commands run from the repository root; environment prefixes below use Bash.

| Command | Outcome |
| --- | --- |
| `cargo test --locked --workspace` | PASS; 2,803 passing tests including doctests. Resource-dependent ignored cases remain excluded. |
| `cargo test --locked -p harbor-terminal --no-default-features` | PASS; renderer-free unit, integration and doc tests. |
| `HARBOR_TEST_GPU=vulkan cargo test --locked --workspace gpu:: -- --ignored --test-threads=1` | PASS; 159 workspace GPU tests. |
| `HARBOR_TEST_GPU=dx12-warp cargo test --locked --workspace gpu:: -- --ignored --test-threads=1 --nocapture` | PASS; 159 workspace GPU tests on the final formatted tree. |
| `HARBOR_TEST_GPU=vulkan cargo test --locked -p harbor-terminal --lib gpu:: -- --ignored --test-threads=1 --nocapture` | PASS; final affected-suite refresh, 54 tests. |
| `cargo test --locked -p harbor-terminal --lib colored_rows` | PASS; CPU compaction test. GPU cases were explicitly exercised by the commands above. |
| `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | PASS on final formatted code. |
| `cargo fmt --all -- --check` / `git diff --check` | PASS. |
| `python scripts/docs/check_docs.py` | PASS after retaining overlay verification in spec 0049 and repairing the initial pre-existing broken references. |

The workspace default command initially timed out during macro compile fixtures; the complete retry exited 0. An old pipeline payload assertion expecting full-slot uploads was adapted to compact nonempty writes and zero removal writes; independent dense-image comparisons preserve presentation coverage. Local raw output is under `target/0050-*.log`; the evidence above does not depend on those ignored files.

No interactive desktop/cross-monitor/external-application acceptance, fresh ConPTY/HMR suite, packaging gate, benchmark, display-present or physical-VRAM-release measurement was run. No numerical speedup, latency improvement, whole-renderer memory bound or immediate driver-release guarantee is asserted. Historical spec 0022 evidence limitations remain unchanged; unrun desktop cases are not promoted to passes.

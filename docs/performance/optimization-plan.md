# Performance Optimization Plan

This plan contains only active or explicitly accepted memory work. Historical captures are in [`memory-baseline.md`](memory-baseline.md), and measurement procedures are in [`profiling-guide.md`](profiling-guide.md).

## Priority

| Item                               | Status    | Reason                                                                          |
| ---------------------------------- | --------- | ------------------------------------------------------------------------------- |
| R1: Reuse renderer scratch buffers | Next      | Per-frame vertex churn was 42.1% of allocations in the reference capture        |
| Re-profile after R1                | Required  | Confirms the new dominant owner before more work                                |
| R2: Grow the glyph atlas on demand | Open      | The fixed atlas reserves 4 MiB of CPU pixels and a fixed GPU texture at startup |
| GlyphKey architecture              | Delivered | Face, glyph ID, size, and style already form stable atlas identity              |
| Tracing registry slab              | Accepted  | Bounded third-party process-lifetime overhead; no Harbor change planned         |

## R1 — Reuse Renderer Scratch Buffers

**Targets:**

- `crates/harbor-terminal/src/render/text.rs`
- `crates/harbor-terminal/src/render/background.rs`
- `crates/harbor-terminal/src/render/decoration.rs`

### Problem

Full and range vertex builders, plus dirty-character collection, create short-lived `Vec` values on dirty frames. The reference capture attributed about 11.6 MiB of cumulative allocation to this pattern.

### Design constraints

- Store reusable scratch vectors in the owning render component.
- Use `clear`, `resize`, or `extend` so capacity grows only for a larger terminal or upload range.
- Preserve current incremental and full-upload decisions.
- Do not make scratch ownership global or shared across independent renderer lifetimes.
- Optimize terminal snapshots separately only if the post-R1 capture makes them material.

### Acceptance

- Renderer allocation count and cumulative bytes decline under the same workload.
- Incremental upload offsets and full-upload behavior remain correct.
- Rendered output is unchanged.
- Standard quality gates pass.

## Re-profile Gate

Immediately after R1:

1. repeat the reference Latin scenario;
2. repeat a dirty-range-heavy output scenario;
3. compare cumulative allocations, allocation count, and live peak;
4. identify the new dominant Harbor-owned allocation path;
5. confirm or revise R2 priority from evidence.

Do not begin a new speculative memory refactor before this gate.

## R2 — Dynamic Glyph Atlas Growth

**Targets:** `crates/harbor-text/src/atlas.rs` and the terminal/widget GPU atlas adapters.

### Problem

The current atlas allocates a fixed 2048×2048 CPU pixel buffer and GPU texture from startup, even for Latin-idle sessions.

### Proposed design

1. start with a smaller atlas, such as 512×512;
2. grow through bounded steps to 2048×2048;
3. repack glyphs when dimensions change;
4. recreate GPU texture and bind group after growth;
5. force a full UV/vertex upload after repacking;
6. preserve incremental tile upload while dimensions remain stable;
7. retain eviction only at the configured maximum.

### Required tests

- placement and used-height behavior at each size;
- UV correctness after repacking;
- CPU/GPU dimension transitions;
- texture and bind-group replacement;
- full-upload invalidation after growth;
- Latin, CJK, and confirmation-window rendering;
- maximum-size eviction.

### Acceptance

- Latin-idle CPU and GPU atlas residency decline.
- Growth and eviction produce no glyph corruption.
- Resize and DPI transitions remain correct.
- The reference heap gate remains below 40 MiB.

## Delivered Glyph Identity: Validation Follow-Up

`GlyphKey` already includes face ID, glyph ID, size, and style. Resolution caching is separate from atlas storage.

Remaining validation:

- deterministic coexistence of multiple fallback faces;
- emoji presentation and variation-selector cases;
- end-to-end visual evidence for primary and fallback faces.

This is test hardening, not a prerequisite for R1 or a reason to redesign atlas identity again.

## Accepted Costs

No action is planned for:

- the bounded `tracing-subscriber` registry slab;
- winit and wgpu global caches;
- swapchain textures replaced during resize.

Reopen an accepted cost only when a comparable capture shows it materially blocks a product memory target.

## Delivery Order

1. `perf(render): reuse vertex scratch buffers`
2. re-profile and record before/after evidence
3. `perf(text): grow the glyph atlas on demand`
4. `test(text): harden fallback-face and emoji GlyphKey coverage`
5. optional complex shaping work as a separate, evidence-backed project

Each implementation unit follows [`../validation.md`](../validation.md) and the scenarios in [`profiling-guide.md`](profiling-guide.md).

# Performance Optimization Plan

This plan records measurement-gated performance work and accepted bounded costs. Historical captures remain in [`memory-baseline.md`](memory-baseline.md); procedures are in [`profiling-guide.md`](profiling-guide.md). [`../current-status.md`](../current-status.md) separates implementation from runtime evidence. Product priority belongs to [`../roadmap.md`](../roadmap.md), with broader performance and memory scope in N15 of [`../next-stage-plan.md`](../next-stage-plan.md).

## Priority

| Item                               | Status                  | Reason                                                                                   |
| ---------------------------------- | ----------------------- | ---------------------------------------------------------------------------------------- |
| Current comparable profile         | Next                    | Establish present allocation owners and budgets before selecting an optimization         |
| R1: Reuse renderer scratch buffers | Open; measurement-gated | Historical render-frame attribution was 42.1%; this is not a current hotspot measurement |
| Re-profile after an optimization   | Required                | Confirms impact and the new dominant owner before selecting further work                 |
| R2: Grow the glyph atlas on demand | Open; measurement-gated | Fixed CPU/GPU atlas allocation still exists; priority depends on new evidence            |
| GlyphKey architecture              | Delivered               | Face, glyph ID, size, and style already form stable atlas identity                       |
| Tracing registry slab              | Accepted                | Bounded third-party process-lifetime overhead; no Harbor change planned                  |

## R1 — Reuse Renderer Scratch Buffers

**Targets:**

- `crates/harbor-terminal/src/render/text.rs`
- `crates/harbor-terminal/src/render/background.rs`
- `crates/harbor-terminal/src/render/decoration.rs`

### Problem

Full and range vertex builders, plus dirty-character collection, still create short-lived `Vec` values on dirty frames; R1 is not delivered. See [text builders](../../crates/harbor-terminal/src/render/text.rs), [background builders](../../crates/harbor-terminal/src/render/background.rs), and [decoration builders](../../crates/harbor-terminal/src/render/decoration.rs).

The historical DirectWrite capture attributed about 11.6 MiB of cumulative allocation to this pattern and 42.1% of total allocated bytes to the capture-era render-frame owner. Those figures are not a profile of `a53395d` or the current tree. Source inspection establishes that allocations remain, not their present cost or rank. First capture the same Latin and dirty-range workloads on a recorded current commit, then decide whether R1 is the next optimization.

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

After the current baseline selects an optimization, repeat these measurements immediately after implementing it (including R1 if selected):

1. repeat the reference Latin scenario;
2. repeat a dirty-range-heavy output scenario;
3. compare cumulative allocations, allocation count, and live peak;
4. identify the new dominant Harbor-owned allocation path;
5. confirm or revise R2 priority from evidence.

Do not begin a new speculative memory refactor before this gate.

## R2 — Dynamic Glyph Atlas Growth

**Targets:** `crates/harbor-text/src/atlas.rs` and the terminal/widget GPU atlas adapters.

### Problem

R2 is not delivered. [AtlasStore](../../crates/harbor-text/src/atlas.rs) still allocates a fixed 2048×2048 CPU pixel buffer (4 MiB); [terminal](../../crates/harbor-terminal/src/render/text.rs) and [widget](../../crates/harbor-widget/src/renderer/widget_text_atlas.rs) adapters create fixed GPU textures. This is a per-atlas fact, not a measurement of total process residency across tabs and windows. Growth remains a candidate only after the current baseline and post-optimization profile justify it.

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

## N15 — Feature Budgets and Lifecycle Scenarios

N15 in [`../next-stage-plan.md`](../next-stage-plan.md) extends the baseline beyond a single Latin terminal. Establish numeric budgets from recorded workloads before accepting feature costs; this plan does not invent current measurements or promise savings.

| Area           | Measure and budget                                                                                                  | Lifecycle check                                                           |
| -------------- | ------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------- |
| Tabs and panes | Incremental live heap/private bytes, atlas/GPU residency, frame and input latency for visible and inactive sessions | Open, switch, split, resize, close, and verify retained resources settle  |
| Font reload    | Peak overlap of old/new fonts, atlases and vertices; reload latency and settled memory                              | Repeated reload plus DPI changes; verify old resources become reclaimable |
| Graphics       | CPU decoded bytes, GPU texture residency, upload volume and frame cost                                              | Replacement, eviction and session close under bounded resource limits     |
| Glass/backdrop | Compositor/GPU cost, resize overhead and idle behavior with effects on/off                                          | Minimize/restore, fallback mode and window close                          |

Record machine, commit, profile, fonts, viewport, session counts and dwell with every result. Use DHAT for allocation evidence, Windows private-memory measurements for process residency, and release/render or GPU/compositor tooling for latency and graphics cost. Profile mixed workloads separately from the historical reference scenario. Threading or renderer restructures require a demonstrated bottleneck and a scoped proposal, not speculative inclusion in R1/R2.

## Delivery Order

1. Capture current comparable Latin/dirty-range baselines and establish N15 feature budgets.
2. Select R1 scratch reuse only if current attribution justifies it; preserve the constraints and acceptance above.
3. Re-profile the same workloads and record before/after evidence, including any regressions.
4. Decide whether R2 atlas growth is warranted from that evidence; implement and re-profile only if selected.
5. Harden fallback-face and emoji `GlyphKey` coverage without redesigning delivered identity.
6. Keep optional complex shaping or thread/renderer restructuring separate and evidence-backed.

Each implementation unit follows [`../validation.md`](../validation.md) and the scenarios in [`profiling-guide.md`](profiling-guide.md).

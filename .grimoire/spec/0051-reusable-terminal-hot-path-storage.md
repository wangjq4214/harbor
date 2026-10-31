# Reusable Terminal Hot-Path Storage

**Spec ID:** 0051
**Status:** Implemented (recorded automated verification; acceptance limits below)
**Date:** 2026-10-31
**Sources:** Owner refinement of item 5 approved allocation reduction in Reader chunks, snapshot row-version/wrap metadata, damage and Sequence scratch, preserving ownership/correctness and bounded reuse without requiring zero-copy. The initial endpoint was spec-only; subsequent owner-authorized implementation, allocation measurements, Release comparisons and Reader experiments are consolidated below. The owner requested moving the three new developer documents' conclusions into the specification before deleting those documents.

## Requirements

### R1 — Reduce repeated allocations in all four hotspots

Reduce avoidable repeated heap allocations after warm-up in:

- **Reader chunks:** reuse storage across PTY chunk delivery/processing where ownership permits, rather than always allocating a fresh payload for each completed read. Removing byte copying itself is not required.
- **Snapshot metadata:** reuse row-version collection storage and soft-wrap projection information where unchanged state and snapshot ownership permit. Reusing cell payloads alone does not address these metadata costs.
- **Damage:** reduce repeated allocation of dirty-range collection storage across update construction and preparation while preserving the same pending damage.
- **Sequence:** reuse temporary row markers, candidate/geometry storage and lookup collections where applicable; retaining the row projection or raster cache alone does not address remaining scratch allocations.

Demonstrate the reduction separately for each category. Reuse must not merely move equivalent repeated temporary allocation to another stage of the same path. This is an additional allocation-efficiency contract, not a claim that the existing incremental-update requirements were unimplemented.

Initialization, capacity growth and storage genuinely needed for new content or independently retained projections may allocate. The target is reduced warmed repeated allocation, not a blanket allocation-free frame or a ban on necessary ownership allocations. Concrete storage layout, pooling mechanism and private API shape remain implementation choices.

### R2 — Preserve ownership, ordering and rendering semantics

- **Reader:** preserve byte contents and exactly-once ordered dispatch. Reused storage must not overwrite bytes still queued or being parsed. Preserve resize ACK epochs, pre-/post-ACK geometry ordering, reader release/recovery and event-driven progress under [ADR 0095](../adr/0095-event-driven-resize-barrier-and-shared-reader-dispatch.md). Reuse must not introduce deadlock or defeat the existing bounded delivery behavior.
- **Snapshots:** retained older projections remain coherent and immutable after later mutations, reads and storage reuse, including wrap metadata. Preserve local row sharing and mutation-based acknowledgement under [ADR 0094](../adr/0094-mutation-revisions-and-incremental-terminal-updates.md) and [spec 0045](./0045-mutation-revisions-and-incremental-terminal-updates.md).
- **Damage:** reading or collecting damage does not consume it. Skipped/failed preparation, stale acknowledgement and reconstruction retain the existing replay/explicit-consumption contract of spec 0045. Reused collection storage must not mutate a previously issued update.
- **Sequence:** preserve retained-row incrementality, actual font/atlas/content dependency invalidation, scalar suppression/fallback, partial uploads and complete-reference-equivalent output under [ADR 0098](../adr/0098-incremental-sequence-presentation.md) and [spec 0048](./0048-incremental-emoji-sequence-rendering.md). Allocation reduction must not restore full-layer work or leave stale geometry/UVs.

### R3 — Keep reuse bounded

Reusable buffers and collections must not grow without bound with historical chunks, edits or sequence identities. Document their capacity/lifetime policy and distinguish reusable scratch from queued Reader payloads and independently retained snapshots. Exercise sustained churn and capacity pressure; demonstrate that completed/released work does not accumulate indefinitely in retained reuse storage. This contract does not select a new numerical memory cap or require shrinking after every operation.

## Solution and integration contracts

Reuse storage at the existing ownership boundaries while keeping independently live data isolated. No particular pool, container or cross-thread zero-copy mechanism is mandated.

| Seam | Exchange | Required guarantee |
| --- | --- | --- |
| PTY reader → ordered event dispatcher/parser | Completed byte chunks and resize ACKs | Payload stays intact until consumption; reuse preserves delivery order, progress and boundedness |
| Screen/snapshot cache → retained update | Row versions, shared cells, wrap metadata and damage | Current coherent projection and immutable old projections; no premature damage consumption |
| Engine damage → renderer preparation/acknowledgement | Pending ranges and preparation result | Same affected content and replay semantics despite collection reuse |
| Sequence preparation → retained rows/resources | Scratch classification, request lookup and geometry results | Scratch reuse cannot corrupt retained associations or bypass dependency invalidation |

Verified source surfaces under `crates/harbor-terminal/src/` are [`io.rs`](../../crates/harbor-terminal/src/io.rs), [`snapshot.rs`](../../crates/harbor-terminal/src/snapshot.rs), [`screen.rs`](../../crates/harbor-terminal/src/screen.rs), [`damage.rs`](../../crates/harbor-terminal/src/damage.rs) and [`render/sequence.rs`](../../crates/harbor-terminal/src/render/sequence.rs). At refinement, Reader sends `buffer[..length].to_vec()`; snapshot construction collects row versions and wrap flags; damage produces an owned range vector; Sequence creates temporary vectors/maps/sets. These observations identify scope, not mandatory edit steps or a final storage design.

## Input-to-outcome verification

| Scenario | Trigger | Required outcome |
| --- | --- | --- |
| Warm Reader traffic | Repeated fixed and varying chunks after storage warm-up; alternate prompt consumption with delayed draining | Byte-for-byte ordered output without corruption or duplication; reduced repeated payload allocations, with necessary capacity/queued ownership allocations distinguished |
| Reader resize and shutdown | Interleave chunks with resize ACK acquisition, resume, timeout and session closure while reuse is active | Existing epoch, geometry-ordering, exactly-once delivery and release/recovery behavior remains correct; no reuse-induced stall |
| Unchanged and local snapshot reads | Repeated reads, then bounded cell/wrap changes; keep an earlier update alive | Reduced avoidable metadata allocations; current output matches a complete reference and all retained older data stays unchanged |
| Damage replay | Accumulate local/structural edits, read repeatedly, skip/fail preparation, attempt a stale ACK, then prepare/acknowledge current state | Collection reuse preserves pending ranges and replay; only valid explicit acknowledgement consumes them |
| Warm Sequence edits | Repeat ordinary and emoji edits on touched rows while unrelated emoji remain visible | Reduced scratch allocations; unchanged rows stay retained and uploads stay incremental; output matches fresh complete preparation |
| Real invalidation and churn | Exercise resize, buffer/view changes, font/atlas invalidation and sustained varying content, then release old work | Correct reconstruction/dependency refresh; necessary growth is reported separately; reuse storage does not accumulate historical work indefinitely |

## Evidence and limits

Record reproducible before/after allocation counts and allocated bytes for each hotspot, separating warm-up, steady state, capacity growth and genuine content/reconstruction allocations. Include unchanged reads, bounded edits, Reader delivery/queue pressure and local Sequence changes; use different viewport sizes and keep old snapshots alive so ownership costs are not hidden. Describe attribution and include connected-path totals to expose costs merely shifted between stages. Wall-clock improvements alone are insufficient.

Record workload, iterations/chunk sizes, build/features, platform, grid dimensions, session count, resource/cache state and source revision/dirty scope. Report reusable capacity/retention behavior under churn alongside allocation reductions. No numerical speedup, latency or allocation threshold is selected; no whole-frame zero-allocation or constant-time claim follows from this work.

Preserve applicable regression gates from specs 0045 and 0048 and Reader resize ordering/recovery checks from ADR 0095. Label evidence PASS / FAIL / NOT RUN / BLOCKED honestly. Source inspection and this spec are not implementation, benchmark or runtime acceptance evidence.

## Decisions and scope history

- **All four hotspots, allocation reduction rather than mandatory zero-copy:** approved in this refinement conversation.
- **Necessary allocation and bounded reuse:** owner-approved initialization/growth/new-content exceptions and no unbounded retained capacity growth; concrete storage remains an implementation choice.
- **Existing correctness boundaries remain:** ADRs 0094, 0095 and 0098; this spec supplements rather than supersedes them.
- **Endpoint history:** the original refinement authorized a short spec only. Subsequent authorized implementation and measurements delivered all four hotspots; a further Reader experiment replaced the first Arc/Drop pool with endpoint-owned batch recycling and direct reads.

No new product requirements or architectural supersession are introduced by consolidating this evidence. The existing ADR ownership boundaries remain authoritative. The implementation and measured outcomes below supplement, rather than replace, this regression contract.

## Implementation and ownership

Evidence recorded on 2026-10-31 is consolidated here from the three developer documents removed at the owner's request. This documentation migration does not rerun their tests or timings. Historical and final Reader results are distinguished explicitly; generated plans/logs are not required to interpret this contract.

- **Reader:** [`io/chunk.rs`](../../crates/harbor-terminal/src/io/chunk.rs) separates producer `ReaderBuffers` from session `ReaderRecycle`. Payloads exclusively own initialized 4096-byte buffers and a valid length, with no per-payload pool handle, Arc clone or Drop return hook. The producer reads directly into the buffer; interrupted unpublished reads return locally. Parsing and protocol replies finish before explicit return. Returns publish in batches of 16, with partial batches flushed before drain yields, including ACK/empty/disconnect paths. Acquisition allocates rather than waiting for recycling; no pool lock spans reads, sends, parsing/replies, platform calls or barrier waits. Ordered dispatch, the 32-event queue, 16-event drain budget, continuation wakes and epoch-qualified barriers are preserved.
- **Snapshots:** [`snapshot.rs`](../../crates/harbor-terminal/src/snapshot.rs) compares row versions directly and refills the existing version vector only when projection changes. [`update.rs`](../../crates/harbor-terminal/src/update.rs) caches only the latest snapshot by Screen mutation stamps. Unchanged reads share it; changed, uniquely owned snapshots lend wrap/damage vectors for refill. Retained old updates force independent ownership, preserving their content and metadata.
- **Damage:** caller-owned collectors refill storage without consuming pending changes. The ACK proof checks the sealed snapshot's ranges and a full-upload flag instead of cloning ranges again. Public owned-returning APIs and public snapshot/damage types remain unchanged. [`render/text.rs`](../../crates/harbor-terminal/src/render/text.rs) borrows incoming ranges when no suppression damage is added, otherwise reuses merge storage.
- **Sequence/Atlas:** [`render/sequence.rs`](../../crates/harbor-terminal/src/render/sequence.rs) reuses touched-row flags, candidates/requests, scalar-usability lookup, suppression sets and geometry scratch. Stable candidates move existing requests rather than rebuilding source strings; marker classification uses stack UTF-8 storage. Resolved placements are compared directly to the current atlas. [`render/color_atlas.rs`](../../crates/harbor-terminal/src/render/color_atlas.rs) reuses active-request storage and iterates pending placement work without a temporary list; deduplication checks equality after matching hashes, so collisions cannot merge requests.

### Capacity and lifetime

- **Reader:** shared and producer-local free lists each cap at 34 buffers; the consumer return batch caps at 16. The conservative cap sum is 84 cached slots / 344,064 payload bytes, separate from queued/current payloads, not measured RSS or a claim all slots are allocated. Free-list vectors are preallocated; overflow payloads are freed outside the lock. Non-4096-byte test payloads are not pooled. Shutdown drops endpoints/payloads. Burst pressure may allocate again; this is not a universal allocation-free guarantee. These caps replace the first implementation's single 34-buffer free pool.
- **Snapshots/damage:** cache one latest projection, not frame history. Row versions/wrap metadata follow viewport size; damage capacity follows the largest required collection during that cache lifetime. Externally retained old updates are caller-owned, not additional pool entries; tests verify immutable old projections and historical release.
- **Sequence:** scratch keys clear per use, flags/lookups are viewport-bounded and construction scratch retains one row's high-water capacity. Grid changes reset scratch/cell sets; empty row geometry releases retained CPU/GPU buffers while shared construction scratch may remain. Text merge storage resets on grid resize.
- **Atlas:** active requests/entries retain the existing 128-admission and source/pixel limits. Replacement/truncation drops obsolete identities; empty content drops all identities. Native/negative-result cache limits remain unchanged.

## Recorded allocation evidence

Baseline: HEAD `70d38981aa730249797eb03d2e1e93b50a6bfaf3` plus the owner's pre-existing NormalBuf copy changes and measurement-only hooks; after: the combined implementation working tree. The NormalBuf optimization is present in both versions and is not credited to Spec 0051.

Host: Windows 11 10.0.26300, i5-13600KF, Rust/Cargo 1.99.1, debug test profile, default renderer features, one session, history disabled in fixtures. Consolas 16px, 96 DPI / scale 1; Vulkan on Radeon RX 7900 XT, driver 26.10.1. Correctness was also checked with DX12 WARP.

The test-only thread-local System allocator probe counts allocation/reallocation calls and requested bytes, not allocator overhead, RSS, GPU memory, other threads or native allocations bypassing Rust's allocator. Setup is excluded from warmed probes but separately reported. Counts below are totals over the stated iterations; the Reader warmed rows also hold for the final implementation.

| Workload | Before calls / bytes | After calls / bytes |
| --- | ---: | ---: |
| Reader, 1024 prompt-consumed 64-byte chunks after first chunk | 1024 / 65,536 | 0 / 0 |
| Reader, 1024 prompt-consumed 4096-byte chunks after first chunk | 1024 / 4,194,304 | 0 / 0 |
| Row-version projection, 128 repeated reads, 24×80 | 128 / 49,152 | 0 / 0 |
| Row-version projection, 128 repeated reads, 80×240 | 128 / 163,840 | 0 / 0 |
| Connected update construction, 128 unchanged reads, 24×80 | 384 / 73,728 | 0 / 0 |
| Connected update construction, 128 unchanged reads, 80×240 | 384 / 195,584 | 0 / 0 |
| Parse bounded A/B edit → read → ACK, 128 cycles, old update retained, 24×80 | 1152 / 768,000 | 515 / 679,200 |
| Same connected local-edit path, 80×240 | 1152 / 2,266,112 | 515 / 2,055,512 |
| Damage, 48 ranges × 128 collections, fresh owned → reused collector | 640 / 380,928 | 0 / 0 |
| Sequence dirty-row replay with stable keycap/emoji, 64 prepares, 24×80 | 384 / 31,360 | 0 / 0 |
| Same Sequence replay, 80×240 | 384 / 34,944 | 0 / 0 |
| Sequence local keycap width changes, 64 prepares, 24×80 | 1351 / 157,456 | 391 / 71,696 |
| Same local Sequence changes, 80×240 | 1346 / 165,776 | 386 / 76,432 |

**Cold/growth costs:** final Reader endpoint/cache/payload setup costs 5 calls / 5488 bytes for either chunk size; 1025 prompt-consumed chunks cost the same as one chunk, so the next 1024 allocate zero. The replaced Arc/Drop pool cost 3 / 4960 cold; the original first chunk cost 1 / 64 or 4096. A separate warmed, variable-length 32-live-payload churn test reports zero allocations for 1024 operations, while pressure tests check caps.

The unchanged public owned damage collector still costs 640 / 380,928 in the table's workload; only internal reusable collection is allocation-free there. Cold one-range collection costs 1 / 96, growth to 48 ranges 4 / 2880, then another collection zero. Cold 24×80 update projection costs 33 / 110,704, including necessary cell/metadata ownership.

Sequence replay and stable scalar-dependency refresh measure zero warmed allocations at both sizes. Local-edit totals include atlas and Rust-side GPU/driver staging and are backend-dependent; initialization remains allocating (observed 1036 / 4,327,051 at 24×80 and 950 / 4,327,941 at 80×240, with FontBook warmer for the second grid). Changed geometry still writes row-local buffers.

Connected local update totals include parser work, changed-row ownership, directory construction, independently owned public damage and ACK. Remaining costs were not shifted out of measurement. Fewer allocation calls alone do not establish lower elapsed time.

## Recorded Release timing evidence

### Method and attribution

Both versions were built before timing with optimization 3, LTO, one codegen unit and panic abort, using the same host, font and Vulkan GPU above. The detached baseline preserves the owner's NormalBuf production copy methods byte-for-byte; the public-API benchmark example is identical. No test allocator probe participates.

Four processes per side/scenario alternate before→after and after→before; tables report the median of process medians. Engine/Text/single-thread Reader use logical CPU 0 (affinity 1); cross-thread Reader allows CPUs 0 and 2 (affinity 5). This is a controlled fixture, not normal application scheduling. Compilation, warm-up and fixture/device/font setup are excluded.

Engine and Reader timings are batch elapsed time divided by operations; batch P95 is not individual-operation tail latency. Text measures individual CPU prepares with `Instant`, excluding GPU submit/wait; 100ns timing quantization limits interpretation of small differences. Results attribute gains to each combined path, not independently isolated contributions of every scratch or allocation change. Percentages cannot be added into an application/FPS speedup.

### Engine: snapshot caching helps unchanged reads most

History is disabled and an old update stays alive. Each process runs three warm-up and eleven measured batches: 10,000 updates for one session, or 2000 iterations × eight updates for eight sessions. Local mode parses a bounded A/B overwrite, reads, acknowledges and drops the update.

| Grid / sessions | Workload | Before µs | After µs | Elapsed-time change |
| --- | --- | ---: | ---: | ---: |
| 24×80 / 1 | Unchanged read | 0.227 | 0.077 | −66.2% |
| 24×80 / 8 | Unchanged read | 0.250 | 0.084 | −66.4% |
| 80×240 / 1 | Unchanged read | 0.404 | 0.077 | −81.0% |
| 80×240 / 8 | Unchanged read | 0.418 | 0.085 | −79.8% |
| 24×80 / 1 | Local parse → read → ACK | 1.470 | 1.415 | −3.8% |
| 24×80 / 8 | Local parse → read → ACK | 1.528 | 1.472 | −3.6% |
| 80×240 / 1 | Local parse → read → ACK | 2.645 | 2.704 | +2.2% |
| 80×240 / 8 | Local parse → read → ACK | 2.727 | 2.765 | +1.4% |

Unchanged reads save about 0.15–0.33µs by avoiding repeated metadata construction. Local-edit allocation calls fall 55.3%, but elapsed time improves only about 4% on the small grid. Large-grid process ranges overlap and paired changes reverse sign: no demonstrated gain, nor a firm general slowdown. Damage/ACK changes have no independently isolated elapsed-time result.

### Text / Sequence: moderate Emoji churn gains

Each process performs 100 warm-up and 700 individually timed prepares per case, all at 80 columns. Ordinary edits toggle x/y on row 0; persistent-emoji fixtures keep emoji on another row; Sequence churn alternates 😀 / 👩‍💻 on row 0. This includes Text preparation, Sequence/atlas and CPU-side GPU writes. P95 is the median of process P95 values.

| Grid / fixture | Workload | Median before → after µs | Median time change | P95 before → after µs |
| --- | --- | ---: | ---: | ---: |
| 24×80 / ordinary | Clean | 1.00 → 0.95 | −5.0% | 1.00 → 1.00 |
| 24×80 / ordinary | Bounded edit | 4.25 → 4.20 | −1.2% | 5.25 → 4.85 |
| 24×80 / ordinary | Sequence churn | 10.90 → 9.70 | −11.0% | 15.70 → 14.00 |
| 24×80 / persistent emoji | Clean | 1.00 → 1.00 | 0.0% | 1.00 → 1.00 |
| 24×80 / persistent emoji | Bounded edit | 4.70 → 4.60 | −2.1% | 5.45 → 5.50 |
| 24×80 / persistent emoji | Sequence churn | 11.30 → 10.15 | −10.2% | 14.20 → 14.25 |
| 80×80 / ordinary | Clean | 1.00 → 1.00 | 0.0% | 1.00 → 1.00 |
| 80×80 / ordinary | Bounded edit | 4.85 → 4.70 | −3.1% | 5.70 → 6.05 |
| 80×80 / ordinary | Sequence churn | 11.45 → 10.10 | −11.8% | 17.70 → 14.80 |
| 80×80 / persistent emoji | Clean | 1.00 → 1.00 | 0.0% | 1.00 → 1.05 |
| 80×80 / persistent emoji | Bounded edit | 5.00 → 4.95 | −1.0% | 5.85 → 6.15 |
| 80×80 / persistent emoji | Sequence churn | 12.00 → 10.55 | −12.1% | 16.55 → 12.85 |

Emoji churn median improves consistently about 10–12%; ordinary/clean differences are near timer/noise scale. Tail results are mixed (24-row persistent-emoji churn P95 does not improve). No universal frame/tail-latency claim follows.

### Reader: historical regression and final replacement

The builder extracts successful production pump/command paths and directly includes production chunk storage; only logging in unexercised error branches is stubbed. Input/order checks, queue size, compiler options and consumers match. Final fixtures include explicit consumer recycling and flush costs; original/Drop-pool consumers use a drop-only shim.

Single-thread batches contain 131,072 chunks with draining every one or 16; cross-thread batches contain 65,536 with real producer/consumer threads, a 32-event queue and wake coalescing. Each process uses two warm-up and eleven measured batches. Synthetic reads measure handoff/copy/reclamation, not OS PTY, VT parsing, UI latency or FPS.

**Historical first Arc/Drop pool versus original allocating Reader:**

| Handoff / chunk size | Before ns/chunk | After ns/chunk | Elapsed-time change |
| --- | ---: | ---: | ---: |
| Single thread, drain 1 / 64B | 54.21 | 65.88 | +21.5% |
| Single thread, drain 16 / 64B | 49.18 | 56.67 | +15.2% |
| Single thread, drain 1 / 4096B | 71.26 | 89.14 | +25.1% |
| Single thread, drain 16 / 4096B | 99.73 | 112.05 | +12.3% |
| Cross-thread / 64B | 228.38 | 279.38 | +22.3% |
| Cross-thread / 4096B | 300.79 | 356.89 | +18.6% |

All four cross-thread pairs regressed; the extra 51–56ns/chunk reduced isolated throughput about 16–18%. Locking/reference counts were plausible contributors, not proven by a causal profile. The owner authorized experiments in response; these numbers do not describe the selected final implementation.

**Final endpoint batching/direct-read implementation versus original `to_vec()`:**

| Scenario | Original ns/chunk | Final ns/chunk | Elapsed-time change |
| --- | ---: | ---: | ---: |
| Single thread, 64B, drain every chunk | 54.18 | 55.46 | +2.4% |
| Single thread, 64B, drain every 16 | 49.41 | 29.59 | −40.1% |
| Single thread, 4096B, drain every chunk | 69.38 | 66.38 | −4.3% |
| Single thread, 4096B, drain every 16 | 93.85 | 61.12 | −34.9% |
| Cross-thread, 64B | 239.85 | 54.53 | −77.3% |
| Cross-thread, 4096B | 306.33 | 188.15 | −38.6% |

Single-thread prompt 64B remains about 1.3ns / 2.4% slower: not an all-scenarios win. Cross-thread process-median ranges were 230.75–248.32ns → 46.30–67.34ns for 64B and 299.47–309.68ns → 163.73–196.66ns for 4096B; all four pairs improve.

Batch recycling while retaining copying (A), then direct read (B), were tried before integration. Separate paired comparisons establish the selected combined design, not locks alone as the cause:

| Comparison / scenario | Before → after ns/chunk | Elapsed-time change |
| --- | ---: | ---: |
| Old per-chunk Arc pool → final, cross-thread 64B | 279.95 → 47.30 | −83.1% |
| Old per-chunk Arc pool → final, cross-thread 4096B | 338.81 → 178.29 | −47.4% |
| A (batch + copy) → final, cross-thread 64B | 54.70 → 41.18 | −24.7% |
| A (batch + copy) → final, cross-thread 4096B | 234.26 → 178.63 | −23.7% |

Each comparison has its own run block; absolute numbers from different blocks must not be mixed as a common baseline. Final integration also reuses interrupted unpublished reads. No experiment switch or unused copying branch remains.

## Recorded verification and limitations

The following are **previously recorded results**, not checks re-executed by this documentation migration.

| Contract / gate | Recorded result |
| --- | --- |
| R1 — four allocation hotspots | PASS: separate allocation counts plus connected parser/update/ACK and Sequence/atlas/GPU totals above |
| R2 — Reader ordering/lifetime/recovery | PASS: queued payload integrity, valid slices after long→short reuse, 100 interrupted reads with stable buffer/ACK order, completed-read-before-ACK, stale epochs, bounded drain/wakes, timeout/interrupt/closure/configuration recovery; actual threaded drain emits exactly 2048 replies for 2048 queries |
| R2 — snapshots/damage | PASS: old wrap/content/ranges immutable, stable snapshot identity, unique refill, cleared damage refresh, stale/synthetic/tampered ACK rejection, damage oracle and structural/replay/independent-reader regressions |
| R2 — incremental rendering | PASS: complete-reference native/GPU comparisons, local tile/missing churn, untouched-row promotion, atlas repack, font/DPI/palette changes, structural/skipped/failed updates and suppression |
| R3 — bounded reuse | PASS: shared/local/pending cap pressure and oversized-payload exclusion, 32-live-payload churn, historical snapshot release, scratch capacity and 128-request atlas admission/churn/truncation |
| Final Reader quality gates | PASS: 65 focused IO tests, core-only 865 unit tests plus integration/doc tests, default workspace tests, strict workspace/all-target Clippy and scoped rustfmt/whitespace |
| Earlier integrated quality gates | PASS: workspace and renderer-free tests, strict all-target and CI-style all-feature Clippy, formatting/whitespace |
| Native presentation | PASS: 59 offscreen/native tests on Vulkan and 59 on DX12 WARP |
| Real PTY integration | PASS: 30 explicitly enabled `pty::` cases across application (8), PTY crate (3), terminal live resize (3) and styled-tail/script integration (16) |
| Release harness | PASS: original 40 timing process runs and 80 retained Reader experiment/final process records exited successfully; final records match executable hashes |
| Review / requirements comparison | Recorded no confirmed blocker in the implemented allocation/correctness and selected Reader scope; this is not a universal performance claim |

An initially empty GPU test selection was corrected and is not counted as evidence. An invalid Reader diagnostic that counted cumulative allocation addresses as peak live storage was replaced with exact progress checks; bounded-container tests establish retention separately.

**NOT RUN / not claimed:** fresh interactive screenshot/clipboard acceptance, release packaging, full CI execution, real-PTY timing or comprehensive causal CPU profiling. No whole-frame zero-allocation, constant-time, full-application speedup or general zero-copy guarantee is implied. Existing public owned APIs, retained ownership forks, growth, new content and reconstruction may allocate. This appendix records implementation and automated evidence, not new owner runtime acceptance or a change to related ADR/spec statuses.

### Reproduction and local evidence

Durable entry points: [benchmark example](../../crates/harbor-terminal/examples/hot_path_timing.rs), [Reader builder](../../scripts/terminal/build_hot_path_reader_timing.py), [alternating runner](../../scripts/terminal/run_hot_path_timings.py), [summarizer](../../scripts/terminal/summarize_hot_path_timings.py), [engine allocation tests](../../crates/harbor-terminal/src/hot_path_tests.rs), [Reader tests](../../crates/harbor-terminal/src/io/allocation_tests.rs) and [Sequence allocation tests](../../crates/harbor-terminal/src/render/sequence/allocation_tests.rs).

From the repository root, representative regression commands are:

```sh
cargo test -p harbor-terminal --lib hot_path_ -- --nocapture --test-threads=1
HARBOR_TEST_GPU=vulkan cargo test -p harbor-terminal --lib hot_path_sequence_allocation_measurements -- --ignored --nocapture --test-threads=1
cargo test -p harbor-terminal --lib io:: -- --test-threads=1
cargo test --workspace
cargo test -p harbor-terminal --no-default-features
HARBOR_TEST_GPU=vulkan cargo test -p harbor-terminal --lib render:: -- --ignored
HARBOR_TEST_GPU=dx12-warp cargo test -p harbor-terminal --lib render:: -- --ignored --test-threads=1
HARBOR_TEST_GPU=dx12-warp cargo test --workspace pty:: -- --ignored --test-threads=1
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo fmt --check
git diff --check
```

For timings, prepare a detached baseline at the recorded HEAD with the unrelated NormalBuf copy changes preserved, identical example/manifest entry and the ignored pinned `third_party/conpty/VERSION` prerequisite. Build both with Cargo Release; save their example executables as `target/hot-path-timing/before.exe` and `after.exe` before shared output is overwritten. Do not reset the implementation. Then:

```sh
python scripts/terminal/build_hot_path_reader_timing.py --before target/hot-path-timing/baseline --after . --out target/hot-path-timing
python scripts/terminal/run_hot_path_timings.py --stage engine
python scripts/terminal/run_hot_path_timings.py --stage sequence
python scripts/terminal/run_hot_path_timings.py --stage reader
python scripts/terminal/run_hot_path_timings.py --stage reader-threaded --affinity 0x5
python scripts/terminal/summarize_hot_path_timings.py
```

Final Reader comparisons use the same commands with `--out target/reader-reuse-experiments/final-vs-original` on builder/runner and `--results target/reader-reuse-experiments/final-vs-original/results` on the summarizer. Other historical comparison directories are `final-vs-arc` and `final-vs-copy`. The old Arc source was frozen at `target/reader-reuse-experiments/arc-pool`; batch-copy/direct-read extracted-pump prototypes are measurement artifacts, not independently buildable product trees.

Raw samples/process ranges/hashes/commands are local generated artifacts in `target/hot-path-timing/results/` and the Reader comparison `results/` directories; builders record fingerprints/options in `reader-build.json`. Temporary test logs used `.tmp-hot-path-*.log`. These ignored artifacts may be absent on another checkout; the durable summaries, methods, limitations and reproduction entry points above do not depend on retaining them.

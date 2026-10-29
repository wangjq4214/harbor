# Read-only intermediate reflow projection with one final cell copy

**Status:** Completed
**Date:** 2026-10-29

## Context

Primary resize currently decodes retained cells into owned logical glyphs, moves those cells into temporary packed rows, then clones the packed rows into the detached final ring. The user selected read-only intermediate projection followed by one final cell copy to avoid redundant payload copying.

The existing content and buffer-specific policies in [ADR 0042](./0042-content-anchors-and-buffer-specific-resize.md), ordinary-tail rules in [ADR 0046](./0046-trim-ordinary-trailing-spaces-during-primary-reflow.md), and ConPTY region policy in [ADR 0052](./0052-conpty-live-primary-producer-geometry-and-styled-tail-clipping.md) remain applicable. Transactional resize must still prepare independent state before the PTY change and commit through an allocation-free, infallible swap.

## Decision

During primary reflow preparation, intermediate logical/layout projections retain read-only source locations and the metadata needed to interpret and place content, not owned copies of source `Cell` payloads. Copy surviving source cells only when materializing the detached final buffer; intermediate logical glyphs and packed rows must not introduce another payload copy.

The final prepared buffer owns its cells independently of the old buffer. Source references or locations are preparation-scoped, not a dependency of committed storage on the old buffer. This is not a persistent position-only grid or a promise of zero-copy resize.

Preserve existing semantics: meaningful blanks and tail trimming/clipping, wide glyphs and generated edge padding, explicit breaks and soft wraps, history eviction and logical identity, cursor/pending-wrap/selection/review projection, ConPTY history/live boundaries, and alternate-screen rectangular behavior. Generated padding and wide continuations still require correct final materialization; the one-copy decision concerns retained source payloads.

## Consequences

- Intermediate projections no longer need duplicate cell payload storage, including payloads later trimmed or evicted.
- Preparation must keep its source valid and read-only until final materialization; preparation or PTY failure must not mutate the old buffer.
- This chooses an ownership/copy boundary, not concrete projection types or an implementation sequence.
- Initial selection was not based on measured performance. Scoped implementation and measurements are recorded below; they do not establish end-to-end GUI latency.

## Sources

- User confirmation: read-only intermediate projection, with one final copy into the new buffer.
- [Logical decoding](../../crates/harbor-terminal/src/logical_content.rs): `decode_row` currently clones cells into logical glyphs.
- [Primary reflow](../../crates/harbor-terminal/src/primary_reflow.rs): `prepare_geometry_with_limits` and `pack_line` build temporary packed rows.
- [Final ring construction](../../crates/harbor-terminal/src/normal_buf.rs): `from_reflowed_rows` clones packed cells into independent storage.


## Scoped implementation and verification (2026-10-29)

- Logical interpretation now supports borrowed payloads while retaining owned decoding for copy/read consumers. Primary geometry uses borrowed logical glyphs and preparation-only `ProjectedCell` locations; padding and wide continuations are materialized only in the final ring.
- The final ring owns `Cell` values and suffix strings independently. History byte accounting operates on projected source text without cloning it; trim/clip/eviction precedes materialization. Final invariant validation reads the detached ring rather than requiring owned temporary rows.
- Resize's subsequent `ContentProjection::build` now consumes a borrowed visitor and retains only offsets/spans, avoiding another discarded cell-payload decode.
- New regression assertions cover source-pointer identity, no intermediate materialization, one final source-cell materialization per retained lead, independent suffix ownership after source destruction, trimmed/clipped/evicted payload exclusion, and failure immutability. Existing scalar-reference differential/property tests cover both viewport policies, row/byte limits, malformed input, and anchor/cursor projections.
- `cargo test --workspace`: 2802 passed, zero failed, 187 ignored (including manual/native diagnostics). `cargo test -p harbor-terminal --no-default-features --lib`: 845 passed, zero failed, six ignored. These runs include transactional failure, saved-primary/alternate, selection, history, and read-model regressions.
- `cargo test -p harbor-terminal --test conpty_styled_tail_resize -- --ignored --test-threads=2`: all ten explicitly enabled real ConPTY cases passed, including pending ASCII/CJK commands, editing/Enter, styled tails, forced-wrap prefixes, and wide-glyph boundaries; the six default cases also passed in the workspace run.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` passed. `cargo fmt --all -- --check`, `git diff --check`, and `python scripts/docs/check_docs.py` also passed. The legacy `scripts/docs/checklist_summary.py` is absent and provided no additional evidence.
- `cargo test -p harbor-terminal --release --lib primary_reflow::perf::preparation_performance -- --ignored --nocapture` passed all 12 fixture combinations with equal prepared results. Example: 4000 history rows, ASCII, 160 -> 80 columns, PullHistory: scalar reference 95,756 us / 207,715,768 peak-live allocated bytes versus borrowed projection 34,652 us / 58,950,184 bytes. The allocator scope excludes source-fixture allocation; timings exclude fixture construction and result destruction. The reference is the retained scalar implementation, not the pre-change optimized path, so these numbers do not isolate this patch's speedup.
- No GUI smoke, non-Windows native session run, or claim that other independently owned snapshots/read results never copy cells is included.

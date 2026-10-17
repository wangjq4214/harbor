# Bounded literal search: contract and T0002 evidence

## Scope and source

- Ticket: [T0002](../.grimoire/spec/0027-revisioned-terminal-read-model-and-literal-search.md#stable-slice-contracts-and-evidence), acceptance A1–A6; [spec 0027](../.grimoire/spec/0027-revisioned-terminal-read-model-and-literal-search.md), headless consumer subset only.
- Base revision: `02250ebbbaced6990eb36a9652476a123a30ba2e` plus the uncommitted changes in `crates/harbor-terminal/src/lib.rs`, `crates/harbor-terminal/src/literal_search.rs`, `crates/harbor-terminal/src/literal_search/tests.rs`, this evidence and T0002/set status updates. The local/ignored plan is `.grimoire/plans/0035-bounded-literal-search-consumer.md`.
- Verification environment: Windows, Rust `1.97.1 (8bab26f4f 2026-07-14)`, Cargo `1.97.1 (c980f4866 2026-06-30)`. Tests use real headless terminal engines and the delivered [T0001 boundary](terminal-read-model.md). No application search UI, native search/navigation smoke or new renderer acceptance is claimed.

This is a historical T0002 consumer record. The later [current-session integration](current-session-search.md) supplies query entry, host scheduling, projection and navigation; [widget text input](widget-input.md) supplies its reusable editor. The exclusions below describe what this consumer slice alone established, not the current absence of those features.

## Delivered API and semantics

The public, GPU-independent `harbor_terminal::literal_search` module has an executable API example.

1. `LiteralSearch::start` validates a borrowed literal/budget before copying it or replacing the current query. It returns a non-cloneable cooperative `SearchJob`; no read or full-history snapshot is performed at start. Queries are exact, case-sensitive UTF-8, without normalization or regex. Empty or over-limit queries and invalid budgets fail explicitly, leaving the current query intact.
2. `SearchJob::step` borrows the engine immutably and reads at most one bounded page through `read_model`. Streaming KMP finds overlapping occurrences across soft wraps and page edges, resetting at real hard breaks (including blank lines). Byte hits expand outward to complete shared source units; repeated hits with identical expanded logical endpoints become one result. For example, searching `e` in `e` plus a combining mark returns that complete unit; searching an emoji component returns the whole retained emoji unit. Offsets remain logical atoms, not UTF-8 bytes, scalars or cell columns. Generated wide-edge padding and dotted-circle cues are absent from the source stream; meaningful spaces are searchable.
3. Results own `ContentRange` row proofs plus unique query identity and initial engine/session, buffer, content generation and revision. They also expose the requested scope, selected row count, scope clipping, observed revision and explicit status. They contain no query copy, history text, mutable cells, parser state, physical coordinates or GPU handles.
4. `LiteralSearch::accept` checks current query and active engine/buffer/generation, rejects stale/cancelled work, and refreshes every match against current row evidence. A single changed/lost range rejects the batch. A resolvable anchor, or restoring the same text after overwriting, is insufficient. Unchanged positive matches can survive unrelated output. Old complete negative coverage is downgraded to partial if the revision changed before publication.
5. Keep `AcceptedSearch` plus a result index for stored search navigation. Call `LiteralSearch::target` immediately before activation; it checks current query/lifetime and text again and returns only neutral navigation data. A bare `ActionTarget` does not authorize a later search action after query cancellation. Re-publication likewise passes the owned results through `accept` again; acceptance is not a permanent lease.

Matching and validation neither execute targets nor acknowledge renderer updates, consume parser events, write PTY input or mutate terminal source/styles. Requested scanning is separate from painting/hover; this ticket adds no frame hook or worker scheduling architecture.

## Bounds, coverage and lifecycle

| Resource/work | Delivered bound and limit behavior |
| --- | --- |
| Query | At most **128 UTF-8 bytes**; one byte pattern and a prefix table of at most 128 entries per running job. The bound also limits a hit to at most T0001's 128 row proofs. Rejected input is not copied. |
| Read/copy per step | One `ReadBudget` page: default **32 rows / 1024 units / 64 KiB payload**; hard maxima **128 rows / 4096 units / 256 KiB payload**. Payload includes copied hyperlink metadata. No full-history string/projection is assembled. |
| Scan per step | At most the copied source bytes, hence at most **256 KiB**. KMP comparisons are at most twice scanned bytes amortized across the stream (a page can inherit up to 127 bytes of prefix state). Each occurrence performs at most 128 joins, each copying at most 128 row proofs. `SearchWork` exposes reads, rows, copied units/payload, scanned bytes, comparisons, suffix refreshes and joins for the last step. |
| Streaming retention | No source strings between steps. At most **128 one-unit proofs** at step boundaries, **129 transiently** when admitting a unit; deque allocation is tested at at most 256 records on the verified toolchain. At most 128 suffix proof refreshes plus one fixed-size checkpoint rebase per step. |
| Results | At most **1024** logical ranges, each with at most 128 row proofs. Reaching the cap stops immediately with `Partial(ResultLimit)`, even if the last retained occurrence might actually be the last match. No silent complete claim. |
| Cancellation | Atomic token set immediately by replacement, `cancel` or owner drop. Checks before/after the synchronous bounded read, every unit, at most **256 scanned bytes** apart within a unit, and at step end. A read or individual bounded range operation cannot be interrupted. Cancellation latency is a work bound, not a wall-clock guarantee. |
| Reclamation/ownership | One ID/token in the gate; no retained prior-query archive. Completed/partial jobs release pattern, suffix and continuation. Cancelled/stale jobs also clear result allocations on observation; an externally delayed job retains only its per-job bounded state until next step/drop. Hosts must drop obsolete jobs/completions and bound their own queues rather than accumulate arbitrarily many owned objects. Allocator overhead/capacity slack is not text payload. |
| Failed read | `Partial(Read(error))`, including indivisible `UnitTooLarge`, invalid scope/content (invalid budgets fail at start). Failed T0001 reads return no page counters; `read_calls` counts the attempt and the submitted budget still bounds the read. No scan or complete no-match is claimed for that failed page. |

Coverage states are intentionally distinct:

- **Running**: explicitly incomplete prefix work, which can be consumed for partial publication, but consuming it ends that job.
- **Complete**: all selected retained scope was scanned at one unchanged revision. Scope clipping is separately exposed; this is not a promise about unrequested rows or future output.
- **Partial(ContentChanged)**: the fixed scope end was reached across explicit consistency points, or a completed result was published after revision change. Positive ranges still require validation; absence of matches is not a current complete no-match.
- **Partial(ResultLimit / Read / PositionLimit)**: scanning stopped at the documented limit/error; coverage is incomplete. Byte-position overflow is checked, never wrapped.
- **Stale**: content generation, retained checkpoint or suffix proof became incompatible. Results are cleared and publication is rejected. Request fresh bounded work explicitly; there is no hidden retry/full-history restart.
- **Cancelled**: obsolete query or closed/dropped owner; cleared on observation and never actionable.

After unrelated output, the job explicitly rebases T0001's continuation and refreshes retained suffix proofs before cross-page matching. The captured scope end never grows. Newly read rows can belong to the new consistency point; already read text cannot be silently substituted. The ongoing-output regression scans 60 captured rows / 270 source bytes over more than 100 bounded steps with new output between steps, returns 30 currently valid matches and partial coverage, and does not restart from the history head.

Width/height/reflow, reset and replacement follow T0001's deterministic generation invalidation, not physical-coordinate reuse. Primary and alternate identities remain separate; an unchanged parked buffer can become eligible again when restored under the same query, but replaced/resized generations cannot. Partial eviction invalidates lost/newly truncated rows; untouched later rows remain distinguishable. Search cannot recover ordinary trimmed tails or approved ConPTY styled overflow.

The gate is not the real application session registry. The subsequent [T0003 integration](current-session-search.md) binds host close/replacement/current-session routing to query cancellation and both gates. A fresh engine already has a distinct session identity. Shell EOF alone is not interpreted as host closure or grounds for discarding retained search content.

## Criterion-to-evidence audit

All focused tests are in `crates/harbor-terminal/src/literal_search/tests.rs` and run without the renderer feature.

| Criterion | Evidence |
| --- | --- |
| A1 — Literal semantics | `paged_soft_wraps_overlap_but_never_join_hard_or_blank_lines`, `meaningful_blanks_are_searchable_but_capacity_and_wide_edge_padding_are_not`, overlapping-literal property oracle. Only bounded read-model fragments are consumed. |
| A2 — Provenance/validation/side effects | `publication_and_activation_revalidate_text_even_while_logical_anchors_survive`, `subset_read_and_matching_leave_source_damage_events_and_parser_state_untouched`, generation and late-completion tests. Checks preserve snapshots, content versions, pending damage, OSC 133 events and an unfinished parser CSI. |
| A3 — Cancellation/lifetime | `late_completions_cannot_replace_new_queries_or_revive_cancelled_targets`, `dropped_gate_and_session_close_replacement_invalidate_jobs_without_an_eof_rule`, `cancellation_after_read_and_during_scan_has_finite_work_and_releases_resources`. Delayed completion, cross-thread token observation, deterministic post-read/256-byte cancellation and allocation release are asserted. |
| A4 — Text/continuation/retention | `unicode_occurrences_expand_to_complete_units_not_bytes_scalars_or_cells`, `long_lines_cross_many_batches_and_keep_retained_suffix_absolute_offsets`, `incompatible_portions_or_changed_suffix_stop_instead_of_silently_joining`, `newly_read_revision_is_explicit_and_failed_reads_remain_partial`, `clipped_resize_tails_are_not_resurrected_by_search`. A maximum-sized query crosses 64 two-column physical rows. |
| A5 — Mutation/progress | `erase_structural_edits_and_ring_reuse_never_leave_actionable_old_text`, `resize_height_reflow_alt_saved_primary_and_reset_obey_boundary_generations`, `repeated_output_rebases_pages_and_suffix_proofs_with_useful_fixed_scope_progress`, overwrite, prefix-eviction and replacement tests. Fresh bounded jobs restore usability after invalidation. |
| A6 — Bounded work/honest coverage | Per-step work/live-record/capacity assertions; `query_limits_read_errors_empty_scope_and_result_caps_have_explicit_coverage`, `complete_negative_coverage_becomes_partial_after_output_and_running_is_never_complete`, `amortized_scan_work_stays_linear_on_repeated_prefixes_and_large_units`. A 4000-byte repeated-prefix stream has linear comparison work; a 4001-byte single unit is either scanned atomically within the read bound or explicitly rejected by a smaller budget. |

Inline complete-change review and acceptance audit found no confirmed blocking defect or unmet T0002 criterion. T0001's read/range/resize contract is unchanged. The implementation adds only the consumer module, its export, focused tests and scoped documentation/status evidence. Planning was useful for the coupled provenance, continuation, cancellation and acceptance seams; no delegation or human checkpoint was added.

## Verification results

Commands were executed from the repository root over the combined implementation (final non-behavioral documentation changes are checked separately):

| Gate | Result |
| --- | --- |
| `cargo test -q -p harbor-terminal --no-default-features --lib literal_search` | PASS — 19 focused tests, including the generated-case oracle. |
| `PROPTEST_CASES=2048 cargo test --locked -q -p harbor-terminal --no-default-features --lib literal_search::tests::paged_ascii_matches_agree_with_overlapping_literal_oracle` | PASS — 2048 generated ASCII multi-line/wrap/page cases versus an independent overlapping-literal oracle. |
| `cargo test --locked -q -p harbor-terminal --no-default-features` | PASS — 740 unit tests, one existing ignored unit test, all active integration tests and both API doctests. |
| `cargo test --locked --workspace -q` | PASS — all active workspace suites, including 1060 terminal unit tests and both terminal API doctests; existing ignored/manual tests remain ignored. Non-fatal incremental-cache access-denied notes for harbor-text/config did not fail the build. |
| `cargo clippy --locked --all-targets --all-features -- -D warnings` | PASS. |
| `cargo fmt --all -- --check`, `git diff --check` | PASS. |
| `python scripts/docs/check_docs.py`, `python scripts/docs/checklist_summary.py` | PASS. |

Early test compilation/fixture assertions were corrected to the actual update/event/color APIs and T0001's checkpoint semantics, then all affected checks were rerun. A not-yet-read row can change at an explicit rebase; old suffix proofs cannot. No production read-model change or weakened stale-text requirement was needed.

## Exclusions and handoff

T0002 is headless consumer delivery only. The subsequent [T0003 integration](current-session-search.md) implements actual query entry, scheduling/queue bounds, current-session host gates, renderer projection and previous/next commands. T0004 retains integrated large-history work/memory measurements and interactive Windows ongoing-output/resize/search evidence. This document does not claim those passes, complete spec 0027 or close the full search/shell work packages. No staging or commit is included.

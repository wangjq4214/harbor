# Bounded terminal read model: contract and T0001 evidence

## Scope and source

- Ticket: [T0001](../.grimoire/spec/0027-revisioned-terminal-read-model-and-literal-search.md#stable-slice-contracts-and-evidence), acceptance A1–A7; [spec 0027](../.grimoire/spec/0027-revisioned-terminal-read-model-and-literal-search.md), boundary subset only.
- Verification date: 2026-10-16. Base revision: `e46952cc0d0e90990754efe2bf432c9900d4f2ea` plus the uncommitted T0001 changes in `crates/harbor-terminal/src/{content_anchor,lib,logical_content,normal_buf,screen,read_model}.rs`, `crates/harbor-terminal/src/read_model/tests.rs`, this evidence and ticket status/coverage updates. The implementation plan is local/ignored at `.grimoire/plans/0034-bounded-versioned-read-model.md`.
- Environment: Windows, Rust `1.97.1 (8bab26f4f 2026-07-14)`, Cargo `1.97.1 (c980f4866 2026-06-30)`. Boundary tests use headless engines; no GPU, interactive application or new ConPTY runtime acceptance is claimed.

## Delivered contract

The public API and executable example are in `harbor_terminal::read_model`:

- `Terminal::read_content(ReadScope, ReadBudget)` directly addresses retained ring row slices. Visible scope captures the displayed viewport (including review); retained scope selects oldest-first history plus live rows. The selected scope/end is fixed across pages. Scope overshoot reports clipping; an out-of-range start fails; an empty scope completes without visiting rows.
- Default budget: **32 rows / 1024 complete text units / 64 KiB payload**. Hard maxima: **128 rows / 4096 units / 256 KiB payload**. Zero or over-maximum budgets fail, not clamp. Payload counts actual UTF-8 source text and copied URI/id bytes, including repeated metadata per unit. Fixed-size record storage is bounded separately by row/unit counts; allocator overhead is not counted as payload. Returned counters report actual visits/units/payload. At most one additional row's metadata is inspected for boundary evidence.
- A successful truncated page has an opaque continuation. An indivisible Unicode unit plus its metadata larger than the budget returns `UnitTooLarge` with the required payload size; it never splits or loops successfully without progress. Missing/corrupt retained invariants return an error, not partial success. Batches do not materialize a whole-history projection.
- Pages own their data and survive engine destruction, but ownership is not validity. Each page and neutral range carries engine session identity, primary/alternate kind, content generation and semantic revision. Repeated numeric line IDs/offsets in another generation/session are not the same content.
- Logical positions reuse the established line ID, atom offset and affinity types. Complete text units include combining/emoji suffixes and assigned cell widths, not byte/scalar offsets. Fragments distinguish logical starts, continued/soft-wrapped text, explicit hard breaks (including empty lines), retained-end and truncated-head evidence. Page boundaries do not insert text or newlines. Styles and safe OSC 8 values are copied as owned values, never mutable registry handles.
- `ContentRange` is the neutral decoration/navigation range. Its private bounded row-version evidence is stronger than a resolvable anchor. `ActionTarget` adds navigation/URI/file data only; it never executes, opens, mutates or renders anything. Hosts remain responsible for payload authorization and actual session/query lifecycle gates in T0003.

### Event and validity rules

| Event | Delivered behavior |
| --- | --- |
| Cell write/append, suffix extension, erase, style/provenance or logical-line edit | Semantic revision advances; any range touching a modified row fails conservatively, including edits outside the range and text changed then restored. Untouched retained row proofs remain valid. |
| Strict continuation after semantic revision change | `StaleContinuation`; incompatible revisions cannot silently concatenate. |
| Explicit continuation rebase | Requalifies only an unchanged retained page-edge checkpoint into a new consistency point. Previous page validity is not implied. The original scope end is not extended. `refresh_content_range` separately validates/requalifies old ranges; joining ranges requires equal versions and adjacent same-line unit offsets. This enables bounded progress through history during ongoing output. |
| Viewport, cursor/palette-only state, renderer invalidation/acknowledgement, skipped/failed drawing | Does not consume or determine content validity; reads leave damage, redraw and parser events pending. No multi-renderer subscription is added. |
| Structural row edits | Modified/moved destination rows are conservatively invalidated. Old physical coordinates are never exposed as durable positions or accepted without row proof and content generation. |
| Full/partial eviction and ring reuse | Lost generations fail. The newly truncated head row is conservatively invalidated; later untouched retained rows remain distinguishable/valid. Absolute atom offsets remain intact. |
| Width/height/reflow commit | Deterministically replaces content generation; all old ranges/continuations fail, even if text/anchors survive. Fresh bounded reads use the existing buffer-specific resize result. Discarded preparation and no-op resize leave live validity unchanged. |
| Primary/alternate switch | Only the active kind/generation is eligible. A parked unchanged buffer becomes eligible again when restored. Replacement/clearing gets a fresh generation; saved-primary/parked-alt resize invalidates their old generations. |
| RIS/reset or buffer replacement | Fresh generation; repeated line IDs or text cannot revive prior evidence. |
| Engine/session replacement | Fresh engine session ID; old data cannot validate against the replacement. Actual host close/replacement routing is T0003, not inferred from shell EOF. |

No storage, parser or resize-retention policy is replaced. Reads expose retained source only; ordinary trimmed tails and approved ConPTY live-primary style-only overflow remain lost. History/non-ConPTY full styled retention and alternate rectangular policy remain unchanged.

## Criterion-to-evidence audit

All tests below are in `crates/harbor-terminal/src/read_model/tests.rs` and run without the renderer feature.

| Criterion | Evidence |
| --- | --- |
| A1 — Bounded reads | `visible_and_history_subsets_are_direct_and_report_coverage`, `subset_does_not_decode_or_copy_unrequested_history`, `hard_limits_and_oversized_units_have_explicit_non_looping_errors`; direct constant-time row lookup and budget-before-copy source inspection. An unrequested malformed row with a huge suffix does not affect a bounded subset read. |
| A2 — Continuity/retention | `long_wrapped_line_pages_preserve_offsets_and_never_invent_newlines`, `explicit_blank_lines_are_fragments_not_capacity_text_or_page_breaks`, `partial_eviction_preserves_absolute_offsets_and_ring_reuse_never_aliases`, `read_boundary_does_not_restore_ordinary_or_conpty_clipped_tails`. |
| A3 — Independent headless readers | `independent_readers_do_not_acknowledge_damage_or_consume_parser_events`; independent continuation clones, unchanged pending render update, retained OSC 133 event and explicit renderer acknowledgement. Existing core update replay tests also pass. |
| A4 — Identity/validity | `stale_continuations_reject_overwrite_even_when_anchors_resolve`, `geometry_reset_replacement_and_buffer_transitions_have_distinct_lifetimes`, `unchanged_retained_ranges_and_explicit_rebase_progress_under_output`. |
| A5 — Mutation/event matrix | `mutations_invalidate_row_proofs_but_not_unrelated_surviving_rows`, prefix/ring-reuse and geometry tests, `discarded_resize_preparation_does_not_invalidate_live_content`. Audit of direct ring cell/metadata mutation paths; original copy/reflow/edit/parser/resize suites pass. |
| A6 — Unicode/metadata/neutral data | `unicode_units_styles_blanks_and_generated_padding_use_shared_semantics`, `bounded_reads_agree_with_shared_decoder_before_and_after_reflow`, `links_are_budgeted_owned_values_safe_after_cleanup_reset_and_engine_drop`, `unsafe_link_opens_never_become_read_metadata_and_exact_registry_limits_fit`, `joining_rebased_pages_requires_explicit_range_refresh_and_bounds_proofs`, `targets_are_versioned_data_without_source_or_renderer_side_effects`. |
| A7 — Boundary compatibility | Core-only build/tests and normal dependency graph exclude wgpu/winit/arboard/harbor-widget. Shared logical line/atom/affinity types, no GPU handles or new OSC parsing; existing OSC 133 tests/events remain intact. Public API documentation includes a passing doctest. |

## Verification results

Executed from repository root over the combined formatted implementation:

| Gate | Result |
| --- | --- |
| `cargo test -q -p harbor-terminal --no-default-features --lib read_model` | PASS — 19 focused tests. |
| `cargo test -q -p harbor-terminal --no-default-features` | PASS — 721 unit tests, one existing ignored unit test, all active integration tests and one API doctest. |
| `cargo tree -p harbor-terminal --no-default-features -e normal --prefix none` | PASS — normal core graph excludes wgpu, winit, arboard and harbor-widget. |
| `cargo test --workspace -q` | PASS — all active workspace tests; existing ignored/manual tests remain ignored. Compiler emitted non-fatal incremental-cache access-denied notes for harbor-text/config. |
| `cargo clippy --all-targets --all-features -- -D warnings` | PASS. |
| `cargo fmt --all -- --check`, `git diff --check` | PASS. |
| `python scripts/docs/checklist_summary.py` | PASS — 855 done / 144 open existing protocol checklist items; not a new global acceptance claim. |
| `python scripts/docs/check_docs.py` | PASS on the final tree. Initially failed on four retired-ticket links in ignored plans `0025`, `0026`, `0028`, `0030`; the user authorized repair, and those links now point to surviving contracts/preserved acceptance in the ticket-set READMEs. |
| Same `check_docs` validator restricted to this evidence, T0001, ticket-set README and local plan 0034 | PASS — language and local links. |

Inline code-risk review and intent/acceptance audit covered the complete tracked diff, both new source files, affected mutation producers, resize preparation/commit, alternate ownership and renderer update callers. No confirmed blocker remains. The generation field is intentionally excluded from existing `NormalBuf` storage/content equality, preserving deterministic comparison of independent detached resize preparations; read validity never uses that equality.

## Exclusions

This is clean boundary-ticket evidence, not acceptance of the whole spec or ticket set. T0002 matcher/query lifecycle, T0003 application acceptance/activation/projection/closed-session routing, and T0004 large-history work/memory measurements and interactive Windows search/resize runtime evidence are not implemented or claimed here. No commit, staging, new architecture decision or global ADR/spec promotion is included.

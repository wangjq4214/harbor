# Windows and Performance Acceptance

**Ticket ID:** T0003
**Source:** [Spec 0020](../../spec/0020-color-emoji-sequence-presentation.md), integrated R1-R7 acceptance; [issue #181](https://github.com/wangjq4214/harbor/issues/181); [Validation Policy](../../../docs/validation.md)
**Status:** Todo

## Goal

Self-contained evidence identifies exactly which supported/unsupported emoji presentations work through actual Harbor/ConPTY, proves clipboard/text/neighbor invariants and resource repaint behavior, and reports comparable ordinary-text costs and quality gates without blanket support claims.

## Affected Surfaces

- Reproducible supported/unsupported Windows workloads and focused test inventory for T0001/T0002.
- `docs/verification/` evidence summaries and behavior/support documentation only where implemented and evidenced.
- Performance baseline/after captures, environment and revision provenance; raw captures remain local or in retrievable CI.
- No new implementation architecture or requirement changes.

## Approach

Prepare named/versioned font/platform cases, commands and capture procedures early. Capture the simple monospace baseline before T0002 changes its path, then repeat the same scenario at the final integrated revision. Separately characterize cold/warm emoji work and bounded cache/atlas churn. Follow the [Profiling Guide](../../../docs/performance/profiling-guide.md); no numeric performance threshold is invented by this ticket.

Use actual Harbor/ConPTY for screenshots and selection/clipboard codepoints. Record supporting font/version and observed native color formats/capabilities, and distinguish intended joined presentation from monochrome and leading-pictograph/missing-glyph fallback. Use known unsupported native cases and controlled failures where necessary to cover fallback deterministically; label synthetic evidence separately from runtime evidence.

Run source-extension and font/DPI/appearance transitions without assuming a settings hot-reload feature. Identify the actual PTY fragmentation exercised: merely delaying writes or running model tests does not prove distinct PTY reads. Keep Spec 0015's historical no-wrap discrepancy an explicit exclusion, not a silently accepted width-policy change.

## Dependencies and Coordination

- **Blocked by:** T0002 for final integrated Windows/after-change acceptance. T0001 is transitively required.
- **Blocks:** None within this set.
- **Parallel work:** Baseline capture, fixtures and procedure preparation can start before T0001/T0002 finish.
- **Coordination risks:** Code changes after capture can invalidate support/performance claims. Record revision and dirty-tree scope; repeat affected evidence at the delivered revision. Return a needed semantic/architecture change to refinement instead of amending this ticket ad hoc.

## Acceptance

- [ ] A named supporting Windows font renders `♥️` and `👩‍💻` as complete intended color sequences through actual Harbor/ConPTY, with screenshots and precise font/OS/version provenance.
- [ ] Unsupported font/format/capability cases show documented visible bounded fallback; exact copied scalars, assigned widths and neighboring content remain unchanged. Complete monochrome is distinguished from incomplete sequence fallback.
- [ ] Fragmented source extension replaces a previously drawn unit; evidence identifies the ingestion/PTY split exercised, commands, expected/observed behavior and transport limitations.
- [ ] Font/session/size and DPI transitions repaint retained text without stale glyphs/failures or copy loss; raster-dependent appearance and resumed/skipped-draw behavior have applicable focused/runtime evidence.
- [ ] Composition and clipping evidence covers oversized artwork, transparent edges, foreground-dependent layers and adjacent comparison content; existing combining/orphan/preedit/selection behavior has scoped regression evidence.
- [ ] The focused shaping/atlas/resource-bound/dirty-range/geometry test inventory and actual results are recorded, including forced repack/overflow and unsupported paths.
- [ ] Comparable simple-monospace before/after captures report scenario, revisions, machine/OS/font/viewport/DPI, relevant shaping/raster calls, prepare/presentation/upload work and memory. Cold/warm emoji costs are separated; no unsupported performance pass threshold is claimed.
- [ ] `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test --workspace`, `python scripts/check_docs.py` and `python scripts/checklist_summary.py` have recorded actual outcomes or explicit unavailability reasons.
- [ ] Evidence under `docs/verification/` includes revision/dirty-tree scope, OS/ConPTY/font/DPI and workload versions, commands/steps, expected/observed results, PASS/FAIL/NOT RUN/BLOCKED, artifacts and exclusions. Raw artifacts remain local or retrievable in CI; summaries do not link local-only captures or expose unrelated private content.
- [ ] Support/status claims describe only the implemented and evidenced font/format/platform scope. Unavailable runtime checks remain NOT RUN/BLOCKED and do not mark issue #181 accepted.

## Out of Scope

Universal emoji/font/platform certification, completion of #153/#176's unrelated evidence gaps, new production behavior, resolving the historical no-wrap discrepancy, and claiming native API research/font-table inspection as Windows runtime acceptance.

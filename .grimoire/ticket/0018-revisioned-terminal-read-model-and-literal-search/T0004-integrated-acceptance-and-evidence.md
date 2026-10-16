# Windows and Large-History Integrated Acceptance Evidence

**Ticket ID:** T0004
**Source:** [Spec 0027](../../spec/0027-revisioned-terminal-read-model-and-literal-search.md), R8/R9, R4 progress, End-to-End Verification and Decisions and Scope Traceability; [validation policy](../../../docs/validation.md), [profiling guide](../../../docs/performance/profiling-guide.md).
**Status:** Todo

## Goal

Establish reproducible evidence for the delivered read/search/application/render path, separating implementation, focused test execution, Windows runtime acceptance and performance observations. This ticket is not permission to claim passes from source inspection, issue closure or ticket creation.

## Affected Surfaces

Focused suites delivered by T0001–T0003; integrated Windows application and ConPTY path; large-history work/memory measurement; self-contained evidence summaries under `docs/verification/`. Any test/instrumentation additions needed for integrated scenarios must preserve the established boundaries. Raw local logs/recordings remain local.

## Approach

Run the spec's full end-to-end matrix on the actual application. Build on earlier tests rather than postpone fundamental correctness here. Measure requested copy/scan work separately from steady-state paint/hover and cancellation/output refresh. Record concrete delivered limits and interactions, without inventing performance thresholds.

Keep Windows ConPTY producer geometry, clipping, cursor and subsequent live input correct; retained-history/non-ConPTY and alternate policies remain distinct under [ADR 0052](../../adr/0052-conpty-live-primary-producer-geometry-and-styled-tail-clipping.md) and [spec 0014](../../spec/0014-content-preserving-resize-reflow.md). New broad shell workflow or release acceptance is not implied.

## Dependencies and Coordination

- **Blocked by:** [T0003](./T0003-current-session-search-integration.md): runtime, real acceptance/activation and prepared-render measurements cannot establish delivery before that integrated path exists.
- **Blocks:** None in this set.
- **Coordination:** Reuse implementation-documented contracts and focused regressions from all producers. Do not silently broaden claims to #159/#160 or change historical evidence/ADR statuses.

## Acceptance

- [ ] **A1 — Integrated scenario matrix (R1–R9):** Record expected versus observed results for every spec End-to-End Verification row: soft wraps versus explicit lines; output races; query completion order; resize/review; eviction/reset; buffer/session routing; Unicode/paged lines; large history. Exercise actual application acceptance **and** stored-target activation, not only a matcher. Include overwrite with a still-resolvable anchor, erase/structural edits, partial-prefix eviction/ring reuse, alternate transitions/saved-primary resize, close/replacement and late/cancelled query results. Preserve earlier focused tests and report their executed results.
- [ ] **A2 — Windows runtime (R3/R4/R5/R6/R7/R9):** Exercise real query entry/highlights/previous-next navigation during ongoing output and repeated narrowing/widening and height changes. Observe useful results, valid remap or deterministic invalidation/fresh work, no stale/wrong-session actions and no live-output/input corruption. Include retained CJK/combining/emoji and long paged logical lines. Respect ordinary-tail and ConPTY styled-overflow exceptions rather than promise restoration. State applications/transports actually exercised and exclusions; do not convert model tests into visual/runtime acceptance.
- [ ] **A3 — Reproducible work/memory evidence (R1/R4/R6/R8):** Populate large retained history; measure requested read/copy/scan batches and query/result retention, then repeated frames/hover without search-relevant changes, cancellation/replacement and repeated work under output. Show work within implementation-documented limits, no complete-history copy/scan on each frame/hover, bounded cancellation/resource retention and useful ongoing-output progress rather than endless full-history restarts. Record workload/history size, sessions, dwell, machine/OS, fonts, viewport/scale, profile/instrumentation and capture locations; separate timing from DHAT allocation measurements and keep comparisons comparable. No new numeric pass threshold is established.
- [ ] **A4 — Evidence identity and honesty (R9):** Under `docs/verification/`, record exact revision/dirty paths, Windows/environment/application/ConPTY versions, executable profile/features, exact commands/steps, expected/observed behavior, PASS / FAIL / NOT RUN / BLOCKED, artifacts and exclusions. Keep raw recordings local and avoid secrets; summaries are self-contained or link retrievable CI artifacts, not local-only files. Prior issue closure/source inspection is not a fresh execution pass.
- [ ] **A5 — Implementation gates and scoped conclusion (R9):** Run and record `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test --workspace`, `python scripts/check_docs.py` and `python scripts/checklist_summary.py` (policy locked variants permitted), plus applicable focused/core-only checks from earlier tickets. Failures/limitations remain explicit; inventory counts are not readiness. Summarize delivered limits, interaction/invalidation behavior and verified scope, linking remaining [#159](https://github.com/wangjq4214/harbor/issues/159) and [#160](https://github.com/wangjq4214/harbor/issues/160) scope. Do not claim this document task implements or closes #175/#159/#160.

## Out of Scope

New product requirements, numeric budgets/default keybindings, mandatory remap/cache/thread design, unrelated shell/pane/release packages, changing ConPTY clipping policy, historical-evidence rewriting or promotion of unexecuted scenarios. This ticket describes future implementation verification; no gates or runtime outcomes are asserted by its creation.

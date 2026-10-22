# Revisioned Terminal Read Model and Literal Search

**Spec ID:** 0027
**Status:** Read model, literal consumer and current-session integration implemented; integrated Windows/performance acceptance pending.
**Sources:** [#175](https://github.com/wangjq4214/harbor/issues/175), initial literal slice of [#159](https://github.com/wangjq4214/harbor/issues/159), approved current-session entry/highlights/navigation. [#160](https://github.com/wangjq4214/harbor/issues/160) supplies future shell compatibility, not full workflow delivery.
**Lifecycle extension:** [Spec 0033](./0033-exclusive-extensible-application-popups.md) and [ADR 0077](../adr/0077-extensible-exclusive-application-popups.md) require search to close and clear its query, results, highlights and pending work when another participating popup replaces it. Reopening starts empty; existing provenance, bounded cancellation and no-stale-publication requirements remain unchanged. This selects behavior, not implementation acceptance.

## Requirements
### R1 — Bounded read-only content
Expose visible rows and explicitly bounded retained history without mutable Screen access. Document snapshot lifetime/consistency point, scope, actual copied size, concrete limits, continuation/truncation and invalidation. Subset reads must not first clone/decode all history or consume parser events/renderer damage.
Oversized logical lines and evicted prefixes require absolute offsets and real start/end/continuation evidence; page edges invent neither text nor breaks. Do not silently concatenate incompatible revisions. Independently usable headless readers must leave pending updates renderable after skipped/failed draw; no multi-renderer subscription design is implied.

### R2 — Identity and revisions
Reads/results/ranges identify engine session, relevant buffer, generation and semantic content revision, distinguishing replacement/reset and primary/alternate even with repeated IDs/offsets. Current tab/ring slot or renderer acknowledgement epoch is insufficient. Document revision advancement/validation separately from drawing, projection and acknowledgement. Query identity/cancellation prevents late obsolete publication.

### R3 — Shared text and coordinate semantics
Use shared logical-line/Unicode text units: soft wraps join, hard boundaries/explicit blank lines do not; meaningful blanks differ from capacity and generated wide-edge padding. Include bounded owned style/hyperlink metadata safe after registry cleanup/snapshot expiry, not mutable registry ownership.
Search spans soft wraps/pages but never hard breaks; byte/scalar/cell offsets are not interchangeable. Map matches to complete text units/logical anchors; synthetic cues are not source. Only retained content is exposed, never restoration of evicted prefixes, ordinary trimmed tails or ConPTY live-primary styled overflow.
Preserve [0014](./0014-content-preserving-resize-reflow.md), [anchors 0042](../adr/0042-content-anchors-and-buffer-specific-resize.md), [Unicode 0044](../adr/0044-unicode-text-unit-and-presentation-width-policy.md), [hyperlink 0035](../adr/0035-cell-linked-bounded-registry-for-osc8-hyperlinks.md) and [ConPTY 0052](../adr/0052-conpty-live-primary-producer-geometry-and-styled-tail-clipping.md).

### R4 — Explicit lifetime and invalidation
| Event | Required outcome |
| --- | --- |
| Append/overwrite/erase/structural edit | Changed text cannot retain actionable old match; reject stale work, preserve/safely refresh meaningful retained matches. Resolvable anchor alone proves no text validity. |
| Width/height/reflow | Logical remap or deterministic invalidation/fresh bounded work; never old physical coordinates. |
| Whole/partial-prefix eviction/ring reuse | Lost targets ineligible; surviving suffix distinguished, no storage alias. |
| Primary/alternate/saved-primary resize | Correct buffer identity, no wrong-active-screen late decoration/navigation; existing resize policy. |
| Reset/buffer replacement | Old generations never revive against coincident IDs/text. |
| Session close/replacement | Cancel/invalidate work and stored targets; no action on closed/replaced session. Shell EOF alone is not required invalidation. |
| Query replacement/cancellation | No obsolete publication/revival; cancellation work/resources bounded. |
Conservative invalidation is permitted but must show useful retained-content progress during continuing output, not endless full-history restart. Deterministic reflow invalidation is not full #159 result preservation.

### R5 — Renderer-neutral decoration and action boundary
Identity/version-qualified terminal-domain logical ranges are projected by the concrete renderer using current visible geometry. Highlights mutate no cells/text/SGR/parser; selection/cursor/preedit remain compatible and damage/event acknowledgement independent.
Neutral action-target data supports search and future URL/file consumers without GPU handles, execution/opening as a read/match side effect or mutable metadata lifetime. Revalidate current query/session/buffer/revision/text/retention before publication and again at stored-target activation, not only first display.
Keep shared logical identity/offset/affinity usable for future OSC 133 positions ([ADR 0037](../adr/0037-osc133-shell-integration-metadata.md)); no second physical-row scheme, new OSC 7/133 parsing or command-block collection.

### R6 — First bounded literal-search consumer
Search current session only through the shared read boundary, not mutable Screen/full-history strings. Bound read/copy/scan batches, queries/results, cancellation and retained queues/resources; document concrete limits and incomplete/error behavior. Partial coverage is not complete no-match.
Provide query entry, valid highlights and previous/next retained-content navigation without corrupting output. Work, publication and navigation share provenance; query edits/cancellation invalidate racing completions.

### R7 — Real application integration
Actual application acceptance and activation revalidate provenance, not only standalone matcher tests. Session switches/late events cannot affect another session. App owns composition/commands/activation; widgets remain generic under [ADR 0064](../adr/0064-widget-and-application-component-ownership.md). Reuse [typed command precedence](../adr/0040-application-command-dispatch-and-keybinding-precedence.md)/[structured bindings](../adr/0041-structured-keybinding-configuration.md), preserving consumed/pass-through and no PTY leakage. No new global bus, renderer abstraction or crate split.
Current reusable editing and result/glass refinements are governed by [0028](./0028-reusable-single-line-text-input.md) and [0029](./0029-search-panel-glass-appearance.md), not the historical append-only entry.

### R8 — Work is bounded outside the render loop
Painting/hover consumes prepared results, never scans/copies complete history each frame. Record reproducible large-history work/memory measurements separating requested scanning, steady-state rendering, cancellation and ongoing-output progress. No new numeric performance threshold.

### R9 — Verification and honest completion
Cover all R4 events, output during work, overwrite at still-resolvable anchors, late-query rejection, long paged soft lines and Unicode cell projection through real application acceptance/activation. Windows search/navigation under output and repeated width/height resize needs runtime evidence, not source/model tests.
Use validation policy and [profiling guide](../../docs/developer/profiling.md): exact revision/dirty paths, environment/Windows/app/ConPTY, profile/features, commands/steps, expected/observed outcomes, artifacts/exclusions and PASS/FAIL/NOT RUN/BLOCKED. Self-contained summaries or retrievable CI artifacts; raw recordings local, no secrets.
Run fmt, all-target/all-feature clippy with warnings denied, workspace tests and check_docs (locked policy variants allowed), plus focused/core-only gates. Inventory is not readiness; code/spec/closed predecessor issues are not runtime acceptance.

## End-to-End Verification
| Scenario | Required observation |
| --- | --- |
| Soft wraps versus explicit lines | Find/highlight/navigate wrapped occurrence, never join distinct hard lines. |
| Output races | Delay result, append/overwrite/erase/structural edit before delivery and after presentation before activation: no stale action; useful progress under ongoing output. |
| Query completion order | Old completion after replacement/cancel cannot replace/revive current results. |
| Resize/review | Navigate history then repeatedly narrow/widen/change height under output: valid remap or invalidate/fresh work, no wrong coordinates/input/output corruption; retain ConPTY geometry/cursor guarantees. |
| Eviction/reset | Whole/partial-prefix eviction/ring reuse/reset while delayed: no lost-target action or alias/revived generation. |
| Buffer/session routing | Alternate enter/exit/saved-primary resize, switch/close/replace and late delivery: only correct live session/buffer; no closed target. |
| Unicode/paged lines | CJK/combining/emoji and multibatch long lines: complete-unit source/projection agreement, no cues/padding/page breaks as source. |
| Large history | Populate, scan, repeated unchanged frames/hover, cancel/replace and repeat under output: documented batch/retention bounds, no per-frame full scan, bounded cancellation, useful progress. |

## Stable Slice Contracts and Evidence
Aliases `0018-revisioned-terminal-read-model-and-literal-search`: T0001 → T0002 → T0003 → T0004. Producers have focused regressions immediately, not deferred to integrated QA; shared files alone create no extra dependency.
| Slice | Contract and preserved acceptance aliases | Evidence/state |
| --- | --- | --- |
| T0001 | R1–R5/R8/R9; A1 bounded/lifetime/limits, A2 continuity/retention, A3 independent headless readers/no damage or event consumption, A4 identity/revisions, A5 full mutation matrix, A6 Unicode/metadata/neutral targets, A7 GPU-independent core/future OSC anchors/API docs | Done, scoped automation; read-model contract |
| T0002 | R2–R6/R8/R9; A1 literal semantics, A2 provenance/publication/activation contract, A3 bounded cancellation/late lifecycle, A4 paged Unicode/retention, A5 mutation/output progress, A6 bounded work/resources/honest coverage | Done, headless automation; literal contract |
| T0003 | R2–R9; A1 usable current-session slice, A2 real publication/activation gates, A3 host session/query lifecycle, A4 neutral current-geometry render/no rescans, A5 commands/ownership/no PTY bytes, A6 integrated progress/invalidation | Done, scoped app/core/GPU automation; current-session integration contract. Its original editor limitations are superseded by 0028, not current constraints. |
| T0004 | A1 full scenario matrix/real gates, A2 Windows runtime, A3 work/memory measurements, A4 evidence identity/honesty, A5 gates/scoped conclusion | Todo; acceptance below remains open. |
Delivered concrete API/limits/invalidation are documented in the linked guides and tracked source/tests. Current generation strategy deterministically invalidates resize/reflow; this is a delivered choice, not a new mandatory algorithm. Original API/thread/cache/layout/shortcut choices were not fixed by the spec.

### Outstanding Integrated Acceptance
T0004 must run every matrix row on the actual application, including stored activation after presentation, preserve/run earlier focused suites and report exact applications/transports/exclusions. Windows must show entry/highlights/previous-next during continuing output and repeated narrowing/widening/height changes, Unicode/long lines, useful results and no stale/wrong-session action or live input corruption. Respect history/non-ConPTY versus producer-live/rectangular retention; discarded tails stay lost.
For large-history A3 record workload/history size, sessions/dwell, machine/OS/fonts/viewport/scale, executable profile/instrumentation and captures. Measure read/copy/scan and query/result retention separately from unchanged frames/hover, cancellation/replacement and output refresh; compare like scenarios, separate timing from DHAT allocations. Do not invent thresholds.
A4/A5 require self-contained/retrievable evidence and all applicable gates, explicit failures/NOT RUN and delivered interaction/limit/invalidation summary. Historical GPU font-replacement comparison intermittently failed then passed without changes; cause remains unproven, not fixed by rerun.
#169/#170/#176 closure at source inspection does not prove fresh acceptance or change ADR history. Completing this slice alone does not close #159/#160; explicitly retain/link their remaining accepted scope. No new material matching/product/architecture decision may be inferred.

## Out of Scope
Regex/advanced case options, global cross-session search, automatic URL/file detection/opening, plugins, mutable Screen, ring/storage replacement/unlimited history, full shell command/directory workflows/new OSC parsing, pane delivery or universal renderer hosting. Preserve [GPU-independent core](../adr/0045-gpu-independent-terminal-core-boundary.md), existing OSC 8 activation and transactional PTY/model resize.

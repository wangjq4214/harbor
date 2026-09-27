# Windows, Dependency and Performance Acceptance

**Ticket ID:** T0005
**Source:** [Spec 0016](../../spec/0016-terminal-engine-wgpu-renderer-boundary.md), [issue #176](https://github.com/wangjq4214/harbor/issues/176), [validation policy](../../../docs/validation.md)
**Status:** Todo

## Goal

Verify the delivered core/renderer/session boundary against baseline behavior and retain reproducible dependency, performance, quality-gate and Windows runtime evidence.

## Affected Surfaces

- **Automated checks:** Core-only and rendered build/test targets, dependency-graph assertions and focused boundary/integration tests.
- **Runtime evidence:** `docs/verification/` or retrievable CI artifacts with one-/multi-session measurements and Windows ConPTY/GPU scenarios.
- **Documentation:** Final ownership map and status of the completed boundary, including any compatibility facade retained only for transitional callers.

## Approach

Capture a reproducible baseline before migration; attribute final measurements and manual checks to the integrated revision after T0004. Report actual status rather than claiming that a scenario passed when not run or not reproducible. Follow the repository validation policy; no new performance threshold is invented by this ticket.

## Dependencies and Coordination

- **Blocked by:** T0004 for final integrated Windows/performance acceptance; baseline collection, test scaffolding and command preparation may begin earlier.
- **Blocks:** None; this is final acceptance for the ticket set.
- **Coordination risks:** Tests or performance evidence from earlier intermediate revisions may not reflect the final app integration. Record revision and dirty scope for every result.

## Acceptance

- [ ] Baseline and final dependency graphs demonstrate that the core-only build/test target excludes wgpu, arboard, winit and `harbor-widget` while the rendered application still builds.
- [ ] Record one- and multi-session before/after performance results with reproducible commands, environment and comparison; do not replace observed results with an unsupported speed claim.
- [ ] Windows runtime evidence covers create, resize, tab switch/hide, close, renderer-init failure where reproducible and sustained output; record expected and observed results, artifacts, OS/ConPTY/GPU versions, revision, dirty-tree scope and known exclusions.
- [ ] Focused tests cover full/incremental upload, resize/DPI, palette, selection, cursor, scrollbar, preedit, wake/frame demand, hidden/failed-draw recovery and PTY teardown; state any remaining gaps explicitly.
- [ ] Run applicable `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test --workspace`, `python scripts/check_docs.py` and `python scripts/checklist_summary.py`; mark each PASS / FAIL / NOT RUN / BLOCKED and explain omissions.
- [ ] The ownership map names engine, session adapter, wgpu renderer, host and teardown, matching the delivered code; documentation does not claim this task makes N12 transport-feasible.

## Out of Scope

- Adding features #158, #159, #163 or #166, or redefining their performance budgets.
- Treating an unrun Windows scenario as a pass or claiming a release from this boundary migration alone.

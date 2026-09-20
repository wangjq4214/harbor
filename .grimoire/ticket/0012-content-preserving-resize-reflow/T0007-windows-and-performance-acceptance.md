# Windows and Performance Acceptance

**Ticket ID:** T0007
**Source:** [Spec 0014](../../spec/0014-content-preserving-resize-reflow.md), GitHub #152/#170
**Status:** Todo

## Goal

Close the first content-preserving reflow package with complete automated gates, reproducible Windows shell/`nvim`/clipboard sessions, large-history cost measurements, and documentation that claims only implemented and evidenced behavior.

## Affected Surfaces

- **Integrated regressions:** Terminal model, pointer/selection/copy, alternate-screen restoration, PTY resize failure, and repeated mixed resize.
- **Windows runtime evidence:** `cmd` or PowerShell, `nvim`, clipboard selection/copy, primary/alternate transitions, and exact ConPTY/application versions.
- **Performance evidence:** Large retained history with colored output and CJK under fixed machine, font, viewport, scale, and build profile.
- **Documentation/decisions:** Durable records under `docs/verification/`, current status/protocol wording, ADR-0018 history, and ADR-0042 implementation status.

## Approach

Run focused and workspace automation against the integrated T0006 revision. Execute reproducible Windows scenarios that narrow, widen, change height, repeat, copy selected content, review scrollback, enter/exit `nvim`, and exercise failure/retry where practical. Record revision and dirty-tree scope, environment and versions, steps, expected/observed results, artifacts, exclusions, and honest PASS/FAIL/NOT RUN/BLOCKED outcomes.

Measure large-history resize latency and memory/cost with a documented fixed workload. Do not invent a pass threshold or claim optimization; retain comparable captures and report observed behavior. Only after implementation and evidence satisfy the accepted scope should documentation replace non-reflow status and update ADR-0018/ADR-0042 without erasing decision history.

## Dependencies and Coordination

- **Blocked by:** T0006 — acceptance must exercise the integrated PTY/model, primary/alternate, selection, and failure path.
- **Blocks:** None; this closes the #152 first-delivery ticket set.
- **Coordination risks:** Evidence becomes stale if implementation changes afterward. Freeze or identify the tested revision and record unrelated dirty-tree files explicitly.

## Acceptance

- [ ] Focused reflow, anchor, selection/copy, capacity, alternate-screen, and failure tests pass for the integrated behavior.
- [ ] `cargo fmt --check` passes.
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` passes.
- [ ] `cargo test --workspace` passes.
- [ ] `python scripts/check_docs.py` and `python scripts/checklist_summary.py` pass.
- [ ] A recorded Windows `cmd` or PowerShell session covers width-only, height-only, combined, repeated, and round-trip resize with retained review/copy behavior.
- [ ] A recorded Windows `nvim` session covers alternate-screen redraw and correctly resized primary restoration.
- [ ] Clipboard evidence covers long colored/CJK content, explicit blank lines, meaningful trailing blanks, selection preservation, and documented eviction invalidation without capturing secrets.
- [ ] Large-history resize latency and memory/cost are recorded with revision, machine, OS, font, viewport, scale, history size, workload, and build profile.
- [ ] Every runtime record states expected and observed results, artifacts, exclusions, and PASS/FAIL/NOT RUN/BLOCKED honestly.
- [ ] Status/protocol documentation claims only verified behavior; remaining N02/search/pane/capacity-configuration scope stays deferred.
- [ ] ADR-0018 is superseded or status-linked without rewriting history, and ADR-0042 is advanced only after accepted implementation evidence.

## Out of Scope

- Fixing unrelated failures discovered during acceptance without a separately scoped issue.
- Claiming native Unix runtime support, full Unicode shaping, search, panes, or unlimited history.
- Performance optimization beyond measuring and reporting this delivery.

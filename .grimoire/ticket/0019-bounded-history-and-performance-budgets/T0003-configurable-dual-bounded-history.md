# Deliver configurable dual-bounded history and coherent live reload

**Ticket ID:** T0003
**Source:** [Spec 0031 R1–R2 and R4](../../spec/0031-bounded-history-and-performance-budgets.md), [ADRs 0072](../../adr/0072-dual-bounded-terminal-history.md)/[0073](../../adr/0073-session-history-accounting-and-text-unit-overflow.md), existing [resize contract](../../spec/0014-content-preserving-resize-reflow.md) and [configuration reference](../../../docs/app/configuration.md).
**Status:** Todo

## Goal

Users can configure independent per-session physical-row and UTF-8 source-content history limits. Existing/future sessions coherently receive accepted live changes; exceeding either limit evicts oldest physical history rows without corrupting newer content, anchors or saved-primary state.

## Affected surfaces

- `harbor-config` settings/validation, `config.example.toml`, user configuration documentation.
- Host acceptance/factory seams in `src/shell.rs`, `src/tab_coordinator.rs`, watched reload and terminal configuration preparation/commit.
- Normal-buffer history, primary/saved-primary resize/reflow, content anchors, selection/review/search-target validity.
- Configuration, terminal and cross-session regression tests.

## Approach

Extend the existing bounded ring/retention and complete-candidate transaction rather than introducing a broad storage rewrite or separate history-only partial reload. Use T0001's measured values for shipping defaults/maxima. History bytes describe retained source text, not allocator/process/GPU residency; live and alternate content remain outside that count.

Keep startup invalid-value recovery and strict manual/watched candidate behavior consistent with established settings policy. Prepare required changes before coherent acceptance so a rejected smaller budget cannot prematurely destroy history or publish future-session settings.

## Dependencies and coordination

- **Blocked by:** T0001's measured defaults, maximum accepted settings and resource rationale for numeric-dependent shipping work.
- **Blocks:** T0004 integrated dual-cap/reload and resource acceptance.
- **Coordination risks:** T0002 shares text/cell/screen surfaces. Consume the same admitted-prefix contract and coordinate accounting/edits; T0002 is not a semantic prerequisite for implementing history-only enforcement. Do not claim whole-process boundedness from this ticket.

## Acceptance

- [ ] Documented, validated settings independently govern each session's primary-history row and source UTF-8 byte counts, using calibrated shipping values. No unlimited-history promise is introduced.
- [ ] Row-only, byte-only and combined overflow evict oldest physical history rows until both caps hold; newest content survives and ordinary text units are not split to fit bytes.
- [ ] Output, reflow, height/width changes and saved primary behind alternate enforce the same history limits. Live/alternate content is excluded from history bytes; alternate remains rectangular without scrollback, retaining existing ConPTY policy.
- [ ] Partial logical-line eviction keeps truncated-head metadata; evicted anchors never alias reused ring slots; endpoint loss invalidates selection, evicted review falls back correctly, and stale search/read targets are handled by existing validity gates.
- [ ] Accepted manual and watched changes apply to active/hidden existing sessions, saved primary and future creation without restarting/recreating sessions or PTYs. Shrink evicts immediately upon successful acceptance; growth cannot restore evicted text.
- [ ] Missing/malformed/invalid and preparation/native failure cases do not cause eviction under rejected budgets, partial publication or future-factory drift. Preserve the existing ADR 0061 recovery-failure qualification without claiming old content/geometry was restored when it was not.
- [ ] Focused tests cover startup/reload policy, success/failure across multiple sessions, mixed font/budget changes, saved-primary/alternate transitions, source-byte accounting with multibyte/sequence text, and repeated ring/resize churn.
- [ ] User/example documentation describes units/scope, calibrated values/validation, effects of reload/reflow/shrink and irrecoverable eviction, and distinguishes history content bytes from measured memory. Evidence and standard gates are recorded; broad runtime/performance acceptance remains T0004.

## Out of scope

Whole-process/GPU hard ceilings, changing normal Unicode admission beyond T0002's approved exception, unlimited/off-grid archival, a speculative parser thread, pane workspace, or unprofiled storage replacement.

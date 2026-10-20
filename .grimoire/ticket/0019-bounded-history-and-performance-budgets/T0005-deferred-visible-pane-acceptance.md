# Deferred follow-up: simultaneously visible pane resource acceptance

**Ticket ID:** T0005
**Source:** [Spec 0031 R5–R6](../../spec/0031-bounded-history-and-performance-budgets.md), [ADR 0073](../../adr/0073-session-history-accounting-and-text-unit-overflow.md), [#166](https://github.com/wangjq4214/harbor/issues/166), [#158 — Pane workspace with independent sessions](https://github.com/wangjq4214/harbor/issues/158).
**Status:** Deferred — blocked on externally delivered pane capability; not part of the current active/hidden-tab acceptance gate

## Goal

Preserve the original multi-visible-session resource/latency acceptance obligation as a named follow-up. Once panes are available, produce reproducible simultaneously-visible-session measurements rather than reusing active/hidden-tab evidence as proof.

## Affected surfaces

Then-current pane/session rendering, input/resize and lifecycle observation; the Spec 0031 performance workload/metric matrix, profiling procedures and durable acceptance evidence. This ticket does not define or implement pane workspace behavior.

## Approach

After the required capability exists, recheck the current revision, measured budget applicability and reproducible workloads. Extend acceptance to simultaneously visible sessions with the relevant visible/hidden combinations. Record the actual pane/grid/layout, font/effects, lifecycle and tool conditions, and distinguish the resulting measurements from the earlier one-visible/hidden-tab captures.

Preserve the same separation between throughput/latency, idle CPU/GPU, frame/upload work, process/native memory and atlas/GPU residency. Use the then-applicable approved/calibrated budgets; do not assume earlier numbers establish performance for a different architecture or workload.

## Dependencies and coordination

- **Blocked by:** External #158 supplying runnable simultaneously visible independent pane sessions.
- **Blocks:** Claims of completed multi-visible-session acceptance, not T0001–T0004's explicitly approved narrower delivery.
- **Coordination risks:** Earlier T0001/T0004 procedures/evidence are useful inputs, not blanket acceptance or an artificial implementation blocker. Coordinate the follow-up with the actual pane owners and preserve #166 linkage.

## Acceptance

- [ ] The pane capability prerequisite is identified; while unavailable, the scenario stays Deferred/BLOCKED, never PASS.
- [ ] Once available, reproducible multi-visible-session captures cover Spec 0031's applicable workload/metric matrix with explicit layout/visibility, revision/configuration/tool metadata, distributions, thresholds and expected/observed outcomes.
- [ ] Evidence includes relevant input/presentation, resize, lifecycle/retirement, memory and GPU/atlas/upload observations rather than merely proving panes render.
- [ ] Summaries/supporting artifacts satisfy R6; missing/failed cases remain explicit. Applicable checks are run or accounted for during future execution.
- [ ] #158/#166 and the current-versus-deferred package scope remain linked in acceptance documentation. Closing current-scope work does not erase this obligation or claim it was already tested.

## Out of scope

Pane implementation, defining pane UX/contracts, automatic issue creation/closure, and treating this deferred task as a prerequisite for the approved current active/hidden-tab delivery.

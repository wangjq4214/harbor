# Bounded history and performance budgets (N15 / #166)

**Source:** [Spec 0031](../../spec/0031-bounded-history-and-performance-budgets.md), [ADR 0072](../../adr/0072-dual-bounded-terminal-history.md), [ADR 0073](../../adr/0073-session-history-accounting-and-text-unit-overflow.md), and [issue #166](https://github.com/wangjq4214/harbor/issues/166).
**Ticket folder:** `.grimoire/ticket/0019-bounded-history-and-performance-budgets/`
**State:** Ready to start T0001; implementation and runtime acceptance are not complete. Numeric-dependent shipping work waits for calibration.

## Overview

Deliver configurable, independent per-session primary-history physical-row and UTF-8 source-content byte ceilings; fixed oversized-text-unit protection; coherent existing/future-session live reload; and reproducible current-scope performance/resource acceptance. Preserve existing reflow, anchors, alternate-screen isolation and ordinary Unicode semantics. Account separately for retained content, grid/metadata storage, temporary overlap, native/font memory and GPU/atlas residency.

The user approved the spec → slice endpoint and the semantics in ADR 0073. This set does not choose numerical defaults, maxima, a protective limit or performance thresholds without measurement. T0001 supplies those values and their rationale before dependent shipping work. Bounded calibration fixtures/probes are permitted so calibration does not depend on shipping configuration being implemented first.

T0001–T0004 cover the current delivery. T0005 is an explicitly deferred follow-up for simultaneously visible panes, linked to [#158](https://github.com/wangjq4214/harbor/issues/158) and #166. It does not block acceptance of the expressly narrowed active/hidden-tab delivery, but cannot be marked PASS or silently dropped when reporting package scope.

## Delivery surfaces

| Surface | Contract / owner |
| --- | --- |
| `crates/harbor-config/src/lib.rs`, `config.example.toml`, `docs/app/configuration.md` | Validated, documented history capacities; established startup recovery and strict complete-candidate reload |
| `src/shell.rs`, `src/tab_coordinator.rs`, `src/configuration_watch.rs`, `crates/harbor-terminal/src/configuration.rs` | Coherent accepted state across existing/hidden/saved-primary and future sessions; no rejected-budget eviction |
| `crates/harbor-terminal/src/{normal_buf,primary_reflow,screen,content_anchor,selection_model}.rs` | Oldest-first dual-bounded history, logical truncation and content-target invalidation |
| `crates/harbor-terminal/src/screen/edit/cell_writer.rs`, `model.rs`, read model/search/render consumers | Bounded text admission and a shared admitted prefix, without normal Unicode loss |
| Existing prepare/parser microbenchmarks, `scripts/profiling/`, `src/telemetry.rs`, terminal/widget rendering and host lifecycle | Reproducible observations; microbenchmarks and heap instrumentation do not substitute for end-to-end timing/residency |
| `docs/developer/profiling.md`, durable evidence and applicable user/status documentation | Repeatable commands, numerical rationale, honest current/deferred acceptance and retrievable confidential-safe artifacts |

These are existing integration surfaces, not a mandated module rewrite, configuration grammar or metrics API.

## Requirements coverage

| Spec 0031 requirement | Tickets |
| --- | --- |
| R1 — independent history bounds, eviction and existing reflow/anchor semantics | T0003; integrated acceptance T0004 |
| R2 — atomic live budget changes | T0003; integrated failure/lifecycle acceptance T0004 |
| R3 — protective oversized-unit exception and ordinary Unicode preservation | T0002; integrated consumer/runtime acceptance T0004 |
| R4 — measurement-derived values and calibration gate | T0001; consumers T0002/T0003/T0004 |
| R5 — comparable workload/metric matrix and evidence-led optimization | T0001 tooling/baseline; T0004 changed-build acceptance; T0005 deferred visible-pane extension |
| R6 — provenance, confidentiality, honest outcomes and explicit deferral | Every ticket's evidence; T0004 current closure; T0005 retained follow-up |

## Blocking dependencies

| Producer | Consumer | Why consumer cannot finish first |
| --- | --- | --- |
| T0001 | T0002 | The fixed protective limit must have a measured value/rationale before shipping implementation can be accepted. |
| T0001 | T0003 | Shipping defaults and maximum accepted capacity values require calibration. |
| T0001 | T0004 | Comparable tooling/baseline and declared thresholds are prerequisites for numerical acceptance. |
| T0002 | T0004 | Integrated acceptance must exercise the actual text-protection behavior, not just a probe. |
| T0003 | T0004 | Integrated acceptance must exercise actual configurable dual bounds and live reload. |
| External #158 pane capability | T0005 | Simultaneously visible panes cannot be executed before pane workspace is available. |

Candidate probes and non-shipping fixture work in T0001 are not blocked by T0002/T0003. T0005 is not a producer for T0001–T0004. T0004's evidence is a useful future comparison, not an artificial semantic blocker for T0005.

## Coordination

| Tickets | Risk | Strategy |
| --- | --- | --- |
| T0002 / T0003 | Shared text storage, history accounting, screen/resize tests | Coordinate the existing admitted-prefix and UTF-8 accounting contracts; isolate edits or sequence shared-file changes. Shared files alone do not add a blocking edge. |
| T0001 / implementation work | Instrumentation can change timing and candidate probes can be mistaken for shipping behavior | Record feature/build/tool modes; keep uninstrumented release timing separate from DHAT and label candidate values/probes. |
| T0003 / T0004 | Reload combined with font-driven geometry and native failure can appear partly applied | Carry the complete-candidate and ADR 0061 failure qualifications into deterministic tests and runtime evidence. |
| All | Premature completion/status claims or stale numerical assumptions | Keep observed/expected, calibrated applicability, current acceptance and pane deferral separate; re-profile before selecting an optimization. |

## Recommended order

1. Execute T0001 and publish usable calibrated values, baseline and reproducible procedures.
2. Execute T0002 and T0003; they may proceed in parallel after calibration with shared-source coordination. Prefer T0002 first when working sequentially.
3. Execute T0004 against the combined result. Failed/unavailable scenarios remain explicit; no invented threshold or favorable rerun establishes success.
4. Keep T0005 deferred until #158 provides the required capability; recheck the then-current build and measurement applicability.

## Ticket index

| Ticket | File | Outcome |
| --- | --- | --- |
| T0001 | [Measurement and calibration](./T0001-measurement-and-calibration.md) | Reproducible baseline/tooling and justified numeric contract |
| T0002 | [Bounded text-unit admission](./T0002-bounded-text-unit-admission.md) | Fixed oversized-unit protection with consistent surviving content |
| T0003 | [Configurable dual-bounded history](./T0003-configurable-dual-bounded-history.md) | Per-session dual caps and atomic live budget changes |
| T0004 | [Windows acceptance and evidence](./T0004-windows-acceptance-and-evidence.md) | Current-scope integrated correctness, performance/resource evidence and documentation |
| T0005 | [Deferred visible-pane acceptance](./T0005-deferred-visible-pane-acceptance.md) | Retained, externally blocked multi-visible-session follow-up |

## Execution and closure guardrails

The current change creates requirements/tickets only. It does not authorize this authoring run to implement, benchmark, perform runtime QA, stage/commit or mutate/close GitHub issues. Execution is a subsequent user-invoked workflow.

Each execution ticket records relevant focused tests and accounts for `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test --workspace`, and `python scripts/docs/check_docs.py`. Record omitted commands and reasons. Removed checklist-summary automation is not a gate. Model/unit tests alone do not establish Windows or performance acceptance.

Evidence follows Spec 0031 R6: durable self-contained summaries, retrievable supporting artifacts, revision/dirty-tree and full relevant environment/workload/tool provenance, expected versus observed results, honest PASS/FAIL/NOT RUN/BLOCKED, and confidentiality. A ticket's bounded delivery is not blanket #166 completion. Package closure requires evidence for accepted scope or explicit linked disposition of remaining/deferred work, including T0005/#158.

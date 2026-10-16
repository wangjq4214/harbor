# Revisioned Terminal Read Model and Literal Search

**Source:** [Spec 0027](../../spec/0027-revisioned-terminal-read-model-and-literal-search.md), including R1–R9, Solution Boundary and Seams, End-to-End Verification, Decisions and Scope Traceability, and Out of Scope.
**Ticket folder:** `.grimoire/ticket/0018-revisioned-terminal-read-model-and-literal-search/`
**Status:** In Progress — T0001 boundary, T0002 headless consumer and T0003 current-session integration delivered with scoped automated verification; T0004 runtime/evidence closure remains Todo. See [T0001 evidence](../../../docs/terminal-read-model.md), [T0002 evidence](../../../docs/terminal-literal-search.md) and [T0003 evidence](../../../docs/current-session-search.md) for results, exclusions and the transient GPU-test observation.

## Overview

Deliver the approved #175 shared read boundary plus the first usable current-session literal-search slice: query entry, highlights and previous/next navigation. The shared boundary is independently verifiable headlessly before its search consumer. Reads, matching, publication, rendering and activation must share logical coordinates and identity/revision provenance without mutable-screen access, renderer damage consumption or full-history work on each frame/hover.

This set consumes the approved conversation scope under refine coordination; it does not introduce requirements or settle material semantic gaps. [#159](https://github.com/wangjq4214/harbor/issues/159) and [#160](https://github.com/wangjq4214/harbor/issues/160) retain their remaining scope: this is not full search/result-preservation or shell command/directory workflow delivery. In particular, spec R4 permits reflow remapping **or** deterministic invalidation with fresh bounded work, but requires useful progress while output continues.

## Source Baseline and Constraints

- Source-inspection revision: `283d55a0606e88bbc3194cb34771fa2378d73490`. Existing logical content/anchors, visible-grid `TerminalSnapshot`, renderer-oriented `TerminalUpdate` and stable `TabId` are foundations, not an established shared revisioned history/search API or acceptance evidence (spec Decisions and Scope Traceability).
- #169/#170/#176 were closed at inspection. Preserve their resulting behavior; do not infer fresh acceptance or revise historical ADR statuses.
- Preserve ring storage, shared Unicode/text-unit semantics, logical anchor identity/offset/affinity, buffer-specific resize and transactional PTY/model geometry. Retained content excludes evicted text, trimmed ordinary tails and approved Windows ConPTY live-primary styled overflow; no restoration promise is added.
- Keep application composition, command meaning and activation policy application-owned; widget mechanisms generic; concrete renderer projection separate from core state. No new crate split, global event bus, universal renderer abstraction, OSC parsing or multi-renderer subscription design is selected.
- API names, scan/thread/cache arrangement, UI placement, shortcuts, matching options and numeric budgets are not selected here. Implementation must document concrete bounded limits, coverage and delivered interactions with tests as required by the spec; return any new material product/domain decision to refine rather than assuming it.
- Session close/replacement makes work and targets ineligible; shell EOF alone is not specified as that invalidation trigger.
- No new CONTEXT/ADR recording is pending or included. No implementation, QA workflow, staging or commits are authorized by this document task.

## Delivery Surfaces and Contract Alignment

| Surface | Delivery owner | Established sources / integration points |
| --- | --- | --- |
| Bounded immutable text, continuity/coverage, content identity/revisions and neutral range/target data | T0001 | `crates/harbor-terminal/src/{logical_content,content_anchor,normal_buf,screen,model,lib,update}.rs`; [ADR 0042](../../adr/0042-content-anchors-and-buffer-specific-resize.md), [0044](../../adr/0044-unicode-text-unit-and-presentation-width-policy.md), [0045](../../adr/0045-gpu-independent-terminal-core-boundary.md) |
| Retained styles/links and resize-loss boundaries | T0001; consumer regressions T0002/T0003 | [Spec 0014](../../spec/0014-content-preserving-resize-reflow.md), [ADR 0035](../../adr/0035-cell-linked-bounded-registry-for-osc8-hyperlinks.md), [0052](../../adr/0052-conpty-live-primary-producer-geometry-and-styled-tail-clipping.md) |
| Bounded literal work, query provenance, cancellation and result validation contract | T0002 | Spec R2–R6/R8/R9; T0001 read/range contracts |
| Application query entry, acceptance/activation, current-session routing and commands | T0003 | `crates/harbor-app/src/{tab_manager,command,workspace,terminal_view}.rs`, root `src/{event,shell,tab_coordinator}.rs`; [ADR 0040](../../adr/0040-application-command-dispatch-and-keybinding-precedence.md), [0041](../../adr/0041-structured-keybinding-configuration.md), [0064](../../adr/0064-widget-and-application-component-ownership.md) |
| Valid-range projection and non-mutating highlights | T0003 | `TerminalWidgetBridge`, `crates/harbor-terminal/src/render/pipeline.rs` and concrete render paths; spec R3/R5/R8 |
| Future shell compatibility, not new command workflows | T0001/T0003 | Shared anchors; [ADR 0037](../../adr/0037-osc133-shell-integration-metadata.md); existing OSC 7/133 metadata |
| Integrated Windows/large-history evidence and implementation gates | T0004 | [Validation policy](../../../docs/validation.md), [profiling guide](../../../docs/performance/profiling-guide.md); future self-contained evidence under `docs/verification/` |

Paths identify affected foundations and likely seams, not a mandate to change every listed file or freeze new module layout.

## Requirement Coverage

| Requirement | Actionable owner and observable coverage | Integrated evidence |
| --- | --- | --- |
| R1 — bounded read-only content | T0001 A1–A3: bounded pages, lifetime/coverage, coherent continuation, independent readers, no damage/event consumption | T0004 A2/A3 |
| R2 — identity and revisions | T0001 A4/A5: session/buffer/generation/content contract; T0002 A2/A3: query and late-result validity; T0003 A2/A3: actual host gates | T0004 A1 |
| R3 — text and coordinates | T0001 A2/A3/A6: retained text/metadata; T0002 A1/A4: literal ranges; T0003 A4: projection | T0004 A1/A2 |
| R4 — lifetime and invalidation | T0001 A4/A5: content events; T0002 A3/A5: stale work and output progress; T0003 A2/A3/A6: host lifecycle and fresh projection | T0004 A1/A2 |
| R5 — neutral decoration/action | T0001 A6/A7: data/lifetime; T0002 A2: provenance; T0003 A2–A5: activation and rendering without mutation | T0004 A1/A2 |
| R6 — first literal consumer | T0002 A1–A6: bounded cancellable matcher; T0003 A1–A3: usable query/highlight/navigation path | T0004 A1–A3 |
| R7 — real application integration | T0003 A1–A3/A5/A6: session routing, actual acceptance/activation and command/input behavior | T0004 A1/A2 |
| R8 — bounded work outside render loop | T0002 A5/A6: bounded work/retention; T0003 A4: prepared render results only | T0004 A3: reproducible work/memory measurements |
| R9 — verification and honesty | T0001 A3–A7, T0002 A3–A6, T0003 A2–A6 own focused regressions immediately | T0004 A1–A5: complete scenario matrix, Windows evidence, gates and explicit limitations |

Acceptance IDs below are local to each ticket. Fundamental regression coverage is not deferred to T0004.

## Dependencies

| Producer | Consumer | Why consumer cannot be implemented/verified first |
| --- | --- | --- |
| T0001 | T0002 | Matcher and headless provenance tests require the bounded, revision-qualified text/continuation/range boundary, not an alternate full-history path. |
| T0002 | T0003 | Actual query publication, decoration and navigation require the consumer's result/provenance, cancellation and validation contract. |
| T0003 | T0004 | Integrated application/runtime and steady-state-render evidence requires the real query, acceptance/activation and renderer path. |

DAG: **T0001 → T0002 → T0003 → T0004**. T0001 is a separately testable shared terminal/metadata boundary, not cosmetic pre-refactoring. No edges arise solely from shared files.

## Coordination and Recommended Order

Execute the dependency chain in order. Source review or evidence-scenario preparation can overlap, but does not complete a blocked behavior/evidence ticket. There are no independent delivery branches in this set.

| Tickets | Coordination risk | Handling |
| --- | --- | --- |
| T0001/T0002/T0003 | Identity, continuation, text-unit ranges and validation must agree end to end | Carry producer contract tests and documented limits into each consumer; do not reinterpret anchors as immutable text or renderer epochs as content validity. |
| T0001/T0003 | Engine/update and bridge/render seams may be touched by both | Keep read/damage acknowledgement separate; coordinate edits at the neutral contract, not by adding an artificial file-based blocker. |
| T0002/T0003 | Cancellation, ongoing-output refresh and session/query ownership meet at delivery | Exercise late completions in the real host path as well as headless tests. |
| T0003/T0004 | Runtime evidence must describe the delivered limits/interactions and exact revision | Retain focused tests from earlier tickets; record exclusions and failures rather than converting scenario preparation into a pass. |

## Ticket Index

| Ticket | File | Outcome |
| --- | --- | --- |
| T0001 | [T0001-bounded-versioned-read-model.md](./T0001-bounded-versioned-read-model.md) | Independently tested bounded immutable read model and neutral range/action contracts |
| T0002 | [T0002-bounded-literal-search-consumer.md](./T0002-bounded-literal-search-consumer.md) | Headless cancellable literal consumer with query/result provenance and validation |
| T0003 | [T0003-current-session-search-integration.md](./T0003-current-session-search-integration.md) | Real current-session query entry, validated highlights and previous/next navigation |
| T0004 | [T0004-integrated-acceptance-and-evidence.md](./T0004-integrated-acceptance-and-evidence.md) | Windows and large-history integrated evidence, quality gates and honest completion |

## Assumptions and Gaps

No new domain facts, acceptance semantics or solution choices are assumed. No blocking gap was identified for this decomposition. Implementation details expressly left open by spec 0027 remain open; a needed new material fact/decision must return to refine. Ticket creation does not implement or close #175/#159/#160, establish runtime passes, or select a downstream workflow.

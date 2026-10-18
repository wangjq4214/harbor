# Command Palette: Existing-Command Delivery

**Source:** [Spec 0030](../../spec/0030-command-palette.md), [issue #156](https://github.com/wangjq4214/harbor/issues/156), approved Spec → Slice endpoint on 2026-10-18.
**Ticket folder:** `.grimoire/ticket/0001-command-palette/`
**Status:** Todo; local execution handoff only, no implementation or runtime acceptance.

## Overview

Deliver a searchable application-command presentation using the existing registry, resolved bindings, reusable editor and typed Host execution. First cover tabs/reload as a usable vertical increment, then complete existing terminal/search/paste actions, then establish integrated Windows evidence and truthful documentation. The whole set is necessary for spec 0030; an increment is not completion of #156.

Profile, pane and appearance feature implementations remain deferred with #156. Do not create working-looking placeholders. When execution starts, create/link bounded implementation sub-issues as #156 requests; local tickets are not native GitHub sub-issue relationships. This artifact-only refinement does not authorize remote issue edits or closure.

## Delivery Surfaces

- `crates/harbor-app/src/command.rs`: registered opener, registry discovery, effective binding lookup and shared availability semantics.
- Application workspace/palette composition: query, selection, disabled reasons, feedback and the existing sole action inbox; reuse `harbor-widget` text/focus/Portal mechanisms without business dependencies.
- `src/shell.rs`, `src/tab_coordinator.rs` and application search/tab integration: Host execution, feedback, result-aware focus and existing confirmation gates.
- Existing widget/app/native routing tests plus targeted PTY-writer captures; only generic routing defects belong in the widget layer.
- User/developer command/configuration/status documentation; retrievable Windows evidence under `docs/verification/` or linked CI artifacts.

Exact module/API names remain implementation choices; this is an outcome decomposition, not an implementation plan.

## Requirement Coverage

| Contract | Ticket ownership |
| --- | --- |
| R1: all implemented visible commands, no placeholders | T0001: tabs/reload increment; T0002: remaining terminal/search actions; T0003: verify complete first-delivery coverage and explicit deferrals |
| R2: opener, filtering, effective bindings | T0001 implementation and focused tests; T0003 native/documentation evidence |
| R3: keyboard/editor/IME contract | T0001 base integration; T0002 coexistence with terminal/search/paste; T0003 native IME acceptance |
| R4: availability/revalidation/feedback | T0001 common mechanism and tabs/reload; T0002 terminal-specific contexts; T0003 integrated observations |
| R5: ownership and one execution path | T0001 establishes first consumer without pre-refactoring; T0002 reuses it; T0003 verifies no divergent behavior |
| R6: no input leakage | T0001 opener/edit/navigation/close; T0002 terminal contexts and confirmation; T0003 real Windows evidence |
| R7: result-aware focus/IME/lifecycle | T0001 terminal/tab/reload and editor lifecycle; T0002 search/confirmation destinations; T0003 integrated lifecycle checks |
| R8: evidence and honest package status | T0001/T0002 focused tests; T0003 quality gates, native evidence and status/remaining-scope handoff |

## Dependencies

| Producer | Consumer | Why consumer cannot finish first |
| --- | --- | --- |
| T0001 | T0002 | Terminal/search integration needs a mounted, invokable palette with the common selection/availability/Host path, rather than a parallel implementation |
| T0001, T0002 | T0003 | Final native acceptance and delivered-scope documentation require the integrated first-delivery binary |

The graph is acyclic. T0003 can prepare scenarios independently, but cannot claim final acceptance before both code slices are integrated. Profile/pane/appearance owners are not blockers for these tickets.

## Coordination

| Tickets | Risk | Strategy |
| --- | --- | --- |
| T0001, T0002 | Shared workspace/command/Host surfaces and availability/focus contracts | Implement T0002 against T0001's actual common path; do not build a second palette/controller/dispatcher |
| T0001/T0002, T0003 | Evidence against an outdated or dirty binary; overstated product status | Record exact revision and dirty scope; re-run affected scenarios after fixes; separate code completion from native acceptance |
| All and future feature owners | Registry growth or changing session capabilities | Revalidate registry/context at execution time; add real commands through the same path; do not silently expand feature scope |

## Recommended Order

T0001 → T0002 → T0003. Keep discovery, execution and their safety tests together in each code slice. Do not split off a speculative registry/editor refactor. Scenario preparation can overlap with implementation, with final runs against the integrated revision.

## Ticket Index

| Ticket | File | Outcome |
| --- | --- | --- |
| T0001 | [T0001-palette-tabs-and-reload.md](./T0001-palette-tabs-and-reload.md) | Usable keyboard palette for existing tabs/reload through the common command path |
| T0002 | [T0002-terminal-actions-and-focus.md](./T0002-terminal-actions-and-focus.md) | Complete existing terminal/search actions with safe availability, clipboard and destination focus |
| T0003 | [T0003-windows-acceptance-and-docs.md](./T0003-windows-acceptance-and-docs.md) | Integrated Windows evidence, quality-gate results and accurate first-delivery/deferred-scope documentation |

## Coordination Contract

Endpoint is Spec → Slice only. Local knowledge/spec/ticket writes are authorized; production changes, running implementation/QA, GitHub mutations, staging and commits are not part of this turn. ADR 0069 and the Command Palette CONTEXT entry were recorded and verified before spec/slice generation; no qualifying record batch remains pending.

Consume the settled contract and established choices; organize/decompose/sequence without inventing domain facts, requirements, constraints, acceptance semantics or architecture. Trace assertions to spec 0030 and its sources. A needed new fact or decision returns to refinement; formatting and task ordering do not authorize filling semantic gaps. Do not independently re-record processed decisions or launch another workflow. No new product assumption is required for this decomposition. Future feature acceptance, exact API spelling, visual styling and unselected behaviors are not supplied by these tickets.

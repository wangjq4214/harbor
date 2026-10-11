# Box Drawing and Decoration Lifecycle

**Source:** [Spec 0022](../../spec/0022-box-drawing-and-decoration-lifecycle.md), [ADR 0056](../../adr/0056-terminal-owned-procedural-box-drawing.md), [Issue #193](https://github.com/wangjq4214/harbor/issues/193), and the user-approved refinement scope of 2026-10-11.
**Ticket folder:** `.grimoire/ticket/0016-box-drawing-and-decoration-lifecycle/`
**Status:** Todo; artifact authoring only, no implementation or new runtime evidence.

## Overview

Deliver the complete U+2500–U+257F block as sharp terminal-local procedural geometry, with seamless compatible joins and bounded retained uploads. Preserve source/width/copy, ordinary/shared text paths, effective colors, conceal, existing SGR semantics, and fixed underline slots. Keep all three decoration kinds aligned through changed font metrics and actual viewport/DPI transitions under the current physical-font-size policy.

Native font acceptance means DirectWrite/wgpu resource replacement with changed metrics through the existing renderer seam, not user-configuration hot reload. Actual cross-monitor acceptance remains distinct. An unavailable required case needs truthful NOT RUN/BLOCKED reporting and an explicit scope decision before completion by deferral.

## Delivery Surfaces

- Terminal renderer: `crates/harbor-terminal/src/render/text.rs`, `pipeline.rs`, `layout.rs`, `gpu.rs`, `mod.rs`, and terminal-local box geometry/resources as needed.
- Decoration and lifecycle: `render/decoration.rs`, `decoration_tests.rs`, `text_tests.rs`, and existing viewport/raster-scale/native font replacement seams.
- Existing host/bridge transitions: `crates/harbor-app/src/terminal_view.rs` and `crates/harbor-widget/src/winit/`, inspected or covered where necessary for actual DPI evidence; no new settings reload feature.
- Native resource-smoke fixtures/examples or tests using the existing terminal/font APIs; actual Windows application workload and cross-monitor procedure.
- Self-contained acceptance record and accurate affected status/coverage links in `docs/`. Historical evidence is not rewritten as a new pass.
- `harbor-text/src/font.rs`, `atlas.rs`, and `metrics.rs` are existing contracts to preserve/verify, not authorization to override box glyphs globally.

## Requirement Coverage

| Spec requirement | Owning tickets | Coverage |
| --- | --- | --- |
| R1 complete block and compatible joins | T0001 | Procedural character repertoire, shared edges, sharp strokes, CPU/GPU seam tests |
| R2 ownership, semantic preservation and retained box updates | T0001 | Terminal-only resources, scalar suppression, color/conceal/clip, source/copy, fixed slots and retained replay |
| R3 decoration metric/viewport invariants | T0002 | Consistent projection, changed-metrics replacement, failure retention, physical-font-size DPI policy |
| R4 degenerate inputs and bounded updates | T0001, T0002 | T0001 owns box geometry/buffer cases; T0002 owns decoration/metric-transition cases; each covers its resize offsets and full/incremental convergence |
| R5 Windows evidence and fresh gates | T0003 | Named workload, native resource replacement, actual cross-monitor evidence or authorized deferral, integrated quality and documentation |

## Dependencies

| Producer | Consumer | Why consumer cannot be completed first |
| --- | --- | --- |
| T0001 | T0003 | Final Windows box-drawing acceptance and combined-tree gates require the implemented procedural path |
| T0002 | T0003 | Final font/decoration/DPI acceptance requires the implemented metric/lifecycle invariants |

Evidence procedures and environment discovery can be prepared before producers finish, but T0003 cannot claim integrated acceptance before both are complete. T0001 and T0002 have no semantic blocking edge: T0002 can verify decoration and changed-metrics behavior using existing text while T0001 implements box presentation.

## Coordination

| Tickets | Risk | Strategy |
| --- | --- | --- |
| T0001, T0002 | Shared `pipeline.rs`, viewport helpers and retained resource tests | Agree existing metric/viewport semantics; sequence shared-file edits or coordinate separate ownership. Shared files are merge risks, not fabricated blockers |
| T0001, T0002 | Separate box slots versus existing 96-vertex underline slots | Keep offsets/capacities independent; add guards in their first real consumer rather than a standalone global refactor |
| T0002, T0003 | Native-resource smoke mistaken for desktop settings reload; simulated scale mistaken for cross-monitor evidence | Keep procedures and evidence labels separate, use actual changed metrics, and preserve explicit missing-case status |
| All | A new semantic/architecture/acceptance choice emerges | Return to refinement before changing the settled spec; record qualifying new knowledge once, not retrospectively after invention |

## Recommended Order

T0001 -> T0002 -> T0003 is the low-coordination default because renderer integration files overlap. T0001 and T0002 may proceed in parallel with explicit shared-file coordination. T0003's final evidence/gates follow both; procedure preparation may overlap earlier work.

## Ticket Index

| Ticket | File | Outcome |
| --- | --- | --- |
| T0001 | [T0001-procedural-box-drawing.md](./T0001-procedural-box-drawing.md) | Complete terminal-local box repertoire, seamless compatible joins and safe retained updates |
| T0002 | [T0002-decoration-metric-lifecycle.md](./T0002-decoration-metric-lifecycle.md) | Consistent decoration geometry and coherent font/viewport/DPI lifecycle |
| T0003 | [T0003-windows-integrated-acceptance.md](./T0003-windows-integrated-acceptance.md) | Truthful Windows workload/font/DPI evidence and integrated quality gates |

## Execution Handoff

The architecture and scope are settled in Spec 0022 and ADR 0056; private tessellation/buffer sizing and numerical guards remain implementation details. No blanket runtime deferral, implementation, QA run, commit, or GitHub edit occurred during refinement. The execution environment's cross-monitor capability is unverified.

Consume the established requirements and solution choices without inventing or revising domain facts, constraints, acceptance semantics, architecture, or unconfirmed assumptions. Trace meaningful choices to this handoff/spec/sources. Ordering or formatting does not authorize filling semantic gaps. Suspend on a needed new fact or decision and return it to refinement; resolve it and record any qualifying new knowledge before resuming. Do not re-record already processed choices.

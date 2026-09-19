# Harbor Roadmap

## Direction

Harbor is Windows-first. The next goal is a reliable daily-use terminal with live configuration and a pane workspace; advanced graphics and glass effects build on that foundation rather than delaying basic correctness.

- **What exists:** [Current Status](current-status.md).
- **What to build and how to bound it:** [Next-Stage Product Plan](next-stage-plan.md).
- **What is supported at protocol level:** [Protocol Checklist](protocol/checklist.md).
- **What counts as verified:** [Validation](validation.md).

The N01–N17 identifiers below refer to work packages in the product plan. They are planning commitments, not claims that implementation has started.

## Delivery Order

| Milestone                          | Outcome                                                                                          | Work packages                                 | State / entry condition                                                          |
| ---------------------------------- | ------------------------------------------------------------------------------------------------ | --------------------------------------------- | -------------------------------------------------------------------------------- |
| M0 — Establish evidence            | Reproducible Windows application matrix, code-page/extension probes, honest baseline             | N03; feasibility portions of N10/N12; N16     | **Next, bounded investigation**; can run alongside M1                            |
| M1 — Preserve content              | Resize reflow, coherent text/coordinate semantics, first Unicode correctness slice               | N01, N02; history semantics from N15          | **Next, primary implementation**                                                 |
| M2 — Configure and discover        | Manual then watched reload, command palette, named profiles                                      | N04, N05, N06                                 | **Planned**; simple live settings can start before font/reflow integration       |
| M3 — Work in panes                 | Independent panes, scrollback search, shell-integration workflows                                | N07, N08, N09; multi-session budgets from N15 | **Planned**; requires stable content anchors and session ownership               |
| M4 — Extend compatibility          | Kitty keyboard, selected terminal gaps, bounded static Kitty graphics                            | N10, N11, N12                                 | **Planned / transport-gated**; independent slices may start earlier after probes |
| M5 — Polish the product            | Unified themes/glass chrome, measured refraction prototype, release hardening                    | N13, N14, N16                                 | **Planned**; lightweight polish and release evidence run throughout              |
| M6 — Expand the platform/workspace | Cross-window movement, restoration, separately designed persistent sessions, native Unix runtime | N17                                           | **Deferred** until the relevant Windows stability and ownership gates pass       |

Performance and resource limits (N15), diagnostics, regression tests, and runtime acceptance (N16) apply to every milestone. They are not a final cleanup phase.

## Dependencies That Matter

```text
text + logical-line + anchor contract
  -> reflow / combining-text correctness
  -> font reload, pane resize, search, command navigation

configuration update boundary + existing command registry
  -> reload command / command palette / profiles

session identity + per-pane allocation and focus
  -> pane workspace -> cross-window/persistence work

ConPTY transport evidence
  -> Kitty keyboard and graphics implementation claims

existing Acrylic + theme tokens
  -> readable glass-style chrome
  -> separately measured refraction prototype
```

These are dependency constraints, not a requirement to finish every item in one row before starting any independent work. In particular, a palette prototype, colors/keybindings reload, compatibility fixes, and modest chrome polish need not wait for all pane features.

## First Three Work Items

1. Define N01/N02 content and coordinate semantics, add resize regressions, then implement reflow. Preserve the existing non-reflow behavior until the replacement has explicit tests.
2. Build the N03/N10/N12 ConPTY probes. Treat `chcp` as a compatibility investigation, and verify Kitty negotiation/data in both directions before promising support.
3. Deliver N04 manual reload for colors and keybindings, retaining the last valid live configuration on errors. File watching and font-resource transitions follow.

Keep one major implementation stream plus one short investigation/polish stream active. Split a package into tickets when it is ready to start, with a named owner and exit evidence. Do not label all future milestones "in progress."

## Release Gates

| Release                               | Required outcome                                                                                                                                                                      |
| ------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Windows development preview           | Explicitly scoped features, passing standard checks, reproducible Windows smoke records, documented unsupported behavior                                                              |
| Windows daily-use release             | Committed correctness/workflow scope accepted; parser safety evidence, stable PTY/input/render lifecycle, measured performance, actionable diagnostics, packaging and dogfood records |
| Extended-protocol / visual increments | Their own capability, permission, memory, interoperability, and fallback evidence; no false protocol advertising                                                                      |
| Native cross-platform release         | Unix PTY implementation and native macOS/Linux acceptance in addition to shared model/protocol tests                                                                                  |

A release must state which plan packages it includes and which it excludes. Full xterm coverage, every Kitty enhancement, session persistence, and liquid-glass refraction are not blanket prerequisites for a usable Windows release. Conversely, optional visual features do not excuse missing correctness or runtime evidence.

## Replacing the Previous P0–P8 Plan

The previous roadmap mixed already-implemented foundations, open features, and unrecorded runtime evidence under repeated "In progress" labels. Its execution ordering is superseded by this roadmap; historical ADRs and tickets remain records of their original decisions.

| Previous area                                        | Current home                                                                                          |
| ---------------------------------------------------- | ----------------------------------------------------------------------------------------------------- |
| P0/P1 — automation and parser safety                 | Current Status, Validation, N16                                                                       |
| P2 — screen semantics                                | N01/N02 and protocol regressions                                                                      |
| P3/P4/P5/P6 — compatibility, replies, strings, input | Existing foundations in Current Status; remaining work in N10/N11/N12; runtime evidence in Validation |
| P7 — Windows daily use                               | N04–N09 and N13–N16, with explicit release scope                                                      |
| P8 — native Unix support                             | M6 / N17, still deferred                                                                              |

Only update implementation status from source and focused evidence. A planning document, configured CI workflow, or checked protocol row is not by itself proof that a Windows runtime or release gate passed.

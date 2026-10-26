# Complete Public Contract and Application Composition Acceptance

**Ticket ID:** T0006
**Source:** [spec 0041](../../spec/0041-widget-api-naming-and-configuration-validation.md) R1–R8, E1–E7, Verification and Limitations; [ADR 0087](../../adr/0087-widget-api-naming-and-validation-boundaries.md); [related file](./README.md).
**Status:** Todo

## Goal

Verify the complete public API and real composition paths of the integrated U4, prove that naming, validation, and configuration migration hold simultaneously, and hand off acceptance evidence from the different layers accurately — rather than declaring completion from searches or partial tests.

## Affected Surfaces

- Public Rust compilation / macro passing-failing / cross-crate and dependency-rename fixtures.
- Cross-ticket regressions for widget mounted/layout/scene, application palette/search/rail/notification, and the terminal bridge/external providers.
- Unified public declarations, acceptance evidence, and a gap list for the current `docs/widget/` tutorials/examples and related READMEs. Production behavior fixes are limited to implementing the existing spec; new semantics go back to refine.

## Approach

This ticket focuses on the final declarations and business paths that only cross-ticket verification can validate; it does not take over the caller migration and local regressions that belong to T0001–T0005.

Review evidence against the spec's requirement/scenario mapping, fill in combined tests/examples, and run the corresponding checks. Historical ADR status or previous doc-test results are not pass evidence for this round.

## Dependencies and Coordination

- **Blocked by:** T0001–T0005 integrated; final old-API rejection and end-to-end behavior must be accepted against the same final snapshot.
- **Blocks:** none.
- **Coordination risks:** per-ticket local passes do not imply the final declaration combination passes, especially for multiple Result setters combined with child_with, the target imports, and mixed Card/Flex/ExternalSurface. Final evidence is bound to the actual snapshot, and existing user documentation changes are not overwritten.

## Acceptance

- [ ] Every target type, constructor, setter, callback, and getter of R1–R3/§7 has a corresponding current public entry/consumer check; no old aliases or synonymous constructors remain, and docs do not present historical old names as the current API.
- [ ] Public compilation proves the root imports, external component, ordinary and final Result nodes, macro child blocks, and cross-crate/dependency-rename cases work; old entries, positioning-only Stretch, leaf blocks/statically provable over-counts, and raw View construction/raw Signal subscription are still rejected.
- [ ] Handwritten/macro input through to the actual mounted result covers replacement/clearing, content Fiber/hooks/subscriptions, key/focus, evaluation/laziness/deferred build; the U1–U3 regression contracts are preserved.
- [ ] In the final integration, E2–E4 all hold: static Result, dynamic per-consumption validation panic, every state/optional color, finite domains/positive widths/zero/extremes, and derived layout errors; there is no intermediate chained auto-unwrap and no new global color restriction.
- [ ] The real application paths for E5–E7 cover glass, notification positioning/close, Flex Stretch, text callbacks/direction/IME priority, scroll request → commit, nested event consumption, and the terminal external provider lifecycle; defaults, command sources, generation, and ownership are unchanged.
- [ ] The relevant widget/macro/application/terminal bridge suites, applicable workspace targets/features, docs/examples, formatting, and the diff have actual check results; missing or failing checks are listed explicitly and do not count as passing.
- [ ] Evidence for actual GPU glass, native focus/IME, and real supported HMR generation teardown is traceable separately as passed/not run/blocked, and helper/mounted tests cannot substitute. Unfinished acceptance obligations remain open; do not claim U4 acceptance is complete.
- [ ] The full R1–R8/E1–E7 requirement-to-evidence mapping and the remaining-risk handoff are clear, U5/U6 or old native acceptance is not automatically marked done, and no new unconfirmed requirement/architecture assumption enters the implementation.

## Out of Scope

New product behavior, unselected APIs, unplanned architecture refactors, U5/U6 acceptance, global Color/gradient/HDR design, or weakening acceptance criteria to close evidence gaps.

## Acceptance Evidence

See [the integrated acceptance record](./T0006-acceptance-evidence.md) for the tested snapshot, R1–R8/E1–E7 mapping, actual command results, and resolved documentation/lint findings and remaining native evidence gaps. U4 acceptance is not complete; status and checkboxes remain unchanged.

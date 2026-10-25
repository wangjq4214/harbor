# U4 — Widget API Convergence

**Source:** [spec 0041](../../spec/0041-widget-api-naming-and-configuration-validation.md); [ADR 0087](../../adr/0087-widget-api-naming-and-validation-boundaries.md); [UI_API_REVIEW.md](../../../UI_API_REVIEW.md) U4/F6–F8, §7; the user confirmed adopting the full mapping, no old aliases, explicit legal domains/error exits/material caps/root imports, and the spec → tickets route.
**Ticket folder:** `.grimoire/ticket/0019-widget-api-convergence/`
**Status:** Todo; only requirement decomposition into tickets is done, implementation and QA have not started.

## Overview

Deliver the full U4 rather than a rename-only change: direct configuration returns an explicit Result; dynamic themes/foreground are validated on every consumption and panic explicitly; named materials keep the existing rejections/caps; positioning-only never exposes Stretch; and every naming/construction/query and import migration preserves runtime/application behavior.

Each implementation ticket is responsible for its own real consumers, regressions, and documentation migration; it must not leave callers broken at compile time for the final ticket. T0006 is the full combined contract and evidence close-out, not the first place anything is validated. U1–U3 are not re-implemented; U5/U6 are outside this ticket set.

## Delivery Surfaces

| Surface | Owner tickets |
| --- | --- |
| theme/style/scalar/solid-color validation, static/dynamic error boundaries, content colors/text callbacks/direction enums | T0001 |
| Named tone/rim, shared validation, material packing/actual glass, recipe migration | T0002 |
| Focus/ActionHandler/ShortcutMap, child_with, shared callback names, Widget getter/scroll request names, root imports | T0003 |
| NotificationCard family, notification validation and positioning, CrossAxisAlignment, true Flex Stretch | T0004 |
| ExternalSurface, draw/schedule/IME names and the terminal/host/HMR consumption seams | T0005 |
| Cross-ticket public compilation/macro hygiene/full application and lifecycle acceptance, documentation contract close-out | T0006 |

All tickets are bound by the application/ownership/lifecycle boundaries of spec R8. `lib.rs`, `widgets/mod.rs`, the application composition files, and docs overlap; after a module rename, the same responsibility follows the target path, and no old-name alias is created.

## Dependencies

| Producer | Consumer | Why the consumer cannot finish first |
| --- | --- | --- |
| T0001 | T0004 | The notification close-style and theme-inheritance paths must consume the same implemented ControlStyle/color validation instead of duplicating another rule set; full notification acceptance depends on that production boundary |
| T0001–T0005 | T0006 | Rejection of all old entries, cross-ticket declarations, business regressions, and evidence close-out must target the final integrated API; a partial migration cannot prove U4 as a whole is complete |

There are no other semantic blocking edges. The fixed spec allows T0001/T0002/T0003/T0005 to be developed independently; T0004 may write its positioning/compile cases first but needs T0001 before completion. Shared files are not a reason to invent blocking relationships.

## Coordination

| Tickets | Risk | Strategy |
| --- | --- | --- |
| T0001, T0003, T0004 | Signature overlap across Button/InteractiveRegion/IconButton, getters, notification colors, and shared validation | Fix semantics per spec; each ticket owns its own mapping, reuses T0001 validation, and coordinates or serially integrates the same files |
| T0001, T0003 | Making `.style(...)` return Result and renaming `.content` to `.child_with` affect the same declarations | Introduce no intermediate chained auto-unwrap; each ticket migrates its callers and rechecks the final Result expressions after integration |
| T0002, T0004 | appearance/notifications share glass recipes and GPU scenes | Migrate only the respective inputs/types, preserving values, clip/paint, and lifetime; serialize or atomically integrate shared changes |
| T0003–T0005 | Root/module exports, out-of-crate calls, macro fixtures, and tutorials | Migrate target names/consumers per ticket and coordinate the export files; T0006 aggregates all target and old-entry rejection evidence |
| All | Branch baseline and the user's existing documentation edits | Check the live worktree at the start; do not overwrite existing changes and do not promise cross-generation identity compatibility from type/tree changes |

## Recommended Order

The recommended order is **T0001 → T0002 → T0003 → T0004 → T0005 → T0006**, validating first and then migrating composition consumers, to reduce same-file conflicts. This is an integration recommendation and adds no semantic dependency to T0002/T0003/T0005.

T0001/T0002/T0003/T0005 may proceed in parallel once file ownership, shared signatures, and a merge arrangement are clear; T0004 completes after T0001; T0006 preparation can start early, but its final pass condition waits for T0001–T0005. The dependency graph is acyclic.

## Requirement Coverage

| Spec outcome | Implementation owner | Combined verification |
| --- | --- | --- |
| R1 full type mapping/no aliases | T0003 (ActionHandler/ShortcutMap), T0004 (Card/Alignment), T0005 (ExternalSurface) | T0006 / E1, E6, E7 |
| R2 construction, slots, getters/request queries | T0003; the IconButton getter follows T0001 | T0006 / E1, E7 |
| R3 all properties/callbacks and direction | T0001 (colors/text/Up-Down), T0003 (Focus), T0004 (detail foreground), T0005 (external providers) | T0006 / E1, E7 |
| R4 raw domains and derived layout errors | T0001 (general appearance), T0004 (notifications) | T0006 / E2–E4, E6 |
| R5 Result / dynamic panic / existing macro boundary | T0001, T0004; T0002 keeps material Result | T0006 / E1–E3 |
| R6 named materials with existing caps | T0002 | T0006 / E5 |
| R7 positioning and root imports/private boundaries | T0004 (positioning), T0003 (public root imports); each type ticket updates its own exports | T0006 / E1, E6 |
| R8 consumers, business, runtime, and evidence boundaries | each of T0001–T0005 | T0006 / E1–E7 |

## Ticket Index

| Ticket | File | Outcome |
| --- | --- | --- |
| T0001 | [T0001-appearance-validation-and-text-contracts.md](./T0001-appearance-validation-and-text-contracts.md) | Appearance validation, color vocabulary, and text/direction contracts wired through |
| T0002 | [T0002-named-backdrop-material.md](./T0002-named-backdrop-material.md) | Named materials keep existing parameters/effects/caps |
| T0003 | [T0003-composition-naming-and-root-imports.md](./T0003-composition-naming-and-root-imports.md) | General composition, query, and import migration keeps slot/focus semantics |
| T0004 | [T0004-notification-positioning-and-cross-alignment.md](./T0004-notification-positioning-and-cross-alignment.md) | Card validation, positioning-only types, and true Flex Stretch separated |
| T0005 | [T0005-external-surface-and-bridge-migration.md](./T0005-external-surface-and-bridge-migration.md) | External draw rename and cross-crate/provider lifecycle migration |
| T0006 | [T0006-public-contract-and-integrated-acceptance.md](./T0006-public-contract-and-integrated-acceptance.md) | Complete API/macro/business and layered evidence close-out |

## Handoff and Execution Boundary

- The outputs currently permitted to be written are CONTEXT/ADR, spec, and tickets; this round has no production code, test changes, implementation plan, test execution, or commit.
- Decided behavior is in spec R1–R8; there are no unconfirmed behavioral assumptions or blocking knowledge questions. The spelling of error types/helpers/named input types is not frozen and cannot be used to change domains, fallback strategy, or ownership.
- New behavioral/architectural choices that emerge during implementation go back to refine; ticket executors must not author unconfirmed requirements. Detailed implementation steps are not pre-frozen by this ticket set.
- The follow-up entry point is the user invoking `grimoire-loop` against this directory starting from T0001; this round does not start that flow.

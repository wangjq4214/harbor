# Named Session Profiles and Quick Save

**Source:** [Spec 0052](../../spec/0052-named-session-profiles-and-quick-save.md), [#157](https://github.com/wangjq4214/harbor/issues/157), and ADRs [0106](../../adr/0106-independent-session-profiles-and-explicit-osc7-launch-policy.md), [0107](../../adr/0107-shared-palette-profile-flow-and-in-window-confirmation.md), [0108](../../adr/0108-explicit-session-preset-snapshots-and-safe-configuration-writes.md).
**Ticket folder:** `.grimoire/ticket/0001-named-session-profiles/`
**State:** Local execution handoff only; no implementation, runtime acceptance, remote issues or commits created.

## Overview

Deliver independent named launch recipes with safe OSC 7 directory inheritance, a shared two-level palette chooser, and quick saving through an input-only palette with an adjacent Save… button followed by an in-window confirmation overlay. Existing sessions and ordinary `[shell]` launches remain unchanged.

Saving captures known source launch state and save-time directory, excludes environment by default, and writes only the selected configuration file without overwriting duplicates/external edits or losing unrelated settings/comments.

This decomposition consumes the owner-approved contract, not permission to invent new requirements. If execution needs a new material product/security decision, return that gap to refinement rather than silently choosing it. No separate implementation-plan artifact is required by this handoff.

## Delivery Surfaces

- Configuration parsing/validation and accepted-generation publication: `crates/harbor-config/src/lib.rs`, existing application reload/factory integration.
- Application commands, palette navigation, popup lifetimes, appearance and session metadata: `crates/harbor-app/src/{command,palette,popups,appearance,tab_manager}.rs`.
- Host launch/capture/persistence coordination: `src/{shell,tab_coordinator}.rs`; platform child creation/quoting in `crates/harbor-pty/src/windows.rs`.
- Tests for real child argv/env/cwd, source capture, queue execution, UI input isolation and non-destructive file writes.
- User/configuration/troubleshooting and developer/status documentation; durable evidence under `docs/verification/` or retrievable CI artifacts.

File names describe existing seams, not mandatory new module/type names.

## Dependencies

| Producer | Consumer | Why the consumer cannot complete first |
| --- | --- | --- |
| T0001 | T0002 | The chooser needs a validated accepted catalog and a typed Host launch operation with directory/failure policy. |
| T0001 | T0003 | Saving needs the profile schema/validation/publication contract and retained per-session launch provenance/initial directory. |
| T0002 | T0004 | Integrated native evidence needs the actual chooser, entry paths and launch/focus behavior. |
| T0003 | T0004 | Integrated native evidence needs the real confirmation, capture and persistence flow. |

T0001 is an enabling slice with two real consumers and independently observable child/configuration outcomes, not a speculative global refactor. T0003 is not blocked by T0002: accepted catalog publication and persistence can be tested without the final chooser; T0004 verifies their combined user flow.

## Coordination

| Tickets | Risk | Strategy |
| --- | --- | --- |
| T0002, T0003 | Both extend palette commands/presentation, popup input lifetime and workspace/Host routing | Agree on shared palette-flow extension and lifetime integration from the existing popup contract; coordinate file ownership or sequence shared-file edits. Do not introduce two competing controllers or a general suspended-popup stack. |
| T0001, T0003 | Launch provenance, profile representation and complete-candidate publication | T0001 supplies the actual session recipe/initial-directory facts and catalog contract; T0003 consumes them, not the latest mutable profile definition or runtime environment. |
| All | Legacy shell fallback, native paste confirmation, portable isolation and secret-safe evidence | Retain regression coverage; save confirmation alone is internal, and explicit selected-file writes do not change automatic config behavior. |

## Recommended Order

1. T0001 establishes and verifies the shared launch/configuration/provenance contract.
2. T0002 and T0003 may proceed in parallel with shared-file coordination; sequential execution is equally valid.
3. T0004 integrates the complete flow, records native evidence and publishes only supported documentation claims.

Tests belong in each delivery slice; T0004 is not a reason to postpone correctness tests. At execution start, obtain authorization to create/link bounded implementation sub-issues under #157 as its tracker requires; these local tickets are not remote issues and this refinement does not authorize remote mutation.

## Requirement Coverage

| Contract | Primary delivery owner | Integrated verification |
| --- | --- | --- |
| R1–R3; E2–E5 | T0001 | T0004 actual child/legacy/directory cases |
| R2/R4; E1/E6/E7 | T0002, consuming T0001 catalog and launch policy | T0004 both entry paths and reload/lifecycle |
| R5; E8/E9 | T0003 | T0004 button/Enter/IME/internal-overlay checks |
| R6–R7; E10–E14 | T0003, consuming T0001 provenance/validation | T0004 save → chooser → restart/launch and conflict/mode cases |
| R8; focused coverage and evidence for all E1–E14 | Each slice supplies its focused results; T0004 owns integrated evidence/status | T0004 quality gates, exclusions and #157 disposition |

## Ticket Index

| Ticket | File | Outcome |
| --- | --- | --- |
| T0001 | [T0001-profile-launch-contract.md](./T0001-profile-launch-contract.md) | Validated independent profiles, safe directory/failure policy, coherent acceptance and retained source launch facts |
| T0002 | [T0002-two-level-profile-chooser.md](./T0002-two-level-profile-chooser.md) | New-session and palette selection through one two-level chooser and one Host execution path |
| T0003 | [T0003-confirmed-quick-save.md](./T0003-confirmed-quick-save.md) | Input-only naming/button, internal confirmation and stable, non-destructive selected-file persistence |
| T0004 | [T0004-windows-evidence-and-documentation.md](./T0004-windows-evidence-and-documentation.md) | Integrated Windows evidence, quality-gate records and truthful user/package documentation |

## Handoff Limits

The exact configuration spelling, command IDs, incidental Rust APIs, input styling measurements and conflict-protection algorithm remain implementation details constrained by spec 0052. No unconfirmed assumption may change independent profiles, directory precedence/locality, save snapshot semantics, credential exclusions, confirmation type or overwrite rules. No product code or runtime checks were performed by creating this set.

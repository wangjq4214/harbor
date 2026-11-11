# Named Session Profiles and Quick Save

**Source:** [Spec 0052](../../spec/0052-named-session-profiles-and-quick-save.md), revised 2026-11-04 (R1–R9, E1–E19); [Session Profile Save](../../CONTEXT.md#session-profile-save); [#157](https://github.com/wangjq4214/harbor/issues/157); ADRs [0106](../../adr/0106-independent-session-profiles-and-explicit-osc7-launch-policy.md), [0107](../../adr/0107-shared-palette-profile-flow-and-in-window-confirmation.md), [0108](../../adr/0108-explicit-session-preset-snapshots-and-safe-configuration-writes.md), [0109](../../adr/0109-environment-aware-project-presets-and-recognized-agents.md), and [0110](../../adr/0110-last-recognized-agent-launch-with-original-arguments.md).
**Ticket folder:** `.grimoire/ticket/0001-named-session-profiles/`
**Revision contract:** Owner's refine handoff: update this relationship file and T0003/T0004 only, preserve the four-ticket structure, and return at the ticket-update endpoint. No implementation, runtime acceptance, remote mutation or commits in this revision.

## Overview

Deliver independent named profiles, unchanged ordinary `[shell]` launches, a shared two-level palette chooser, and environment-aware quick save: capture → review/correction → safe persistence → environment/project/agent relaunch. Naming uses an input-only palette with an adjacent Save… button; final confirmation is internal to the existing window (R1–R7/R9).

First-version capture covers local pwsh, WSL bash and direct-SSH bash with guided shell integration. Capture the current project directory at Save initiation in the identified environment; unknown/unusable recognition is visible and correctable, never silently replaced by the initial directory. Remember the last recognized pi, Codex, Claude Code or OpenCode invocation with original arguments, even after exit, scoped to the terminal and execution environment. Reopening enters the saved environment and directory before launching the selected agent; preserve original resume options and add none (R6/R9; ADR 0110).

The draft stays stable after capture except for explicit user corrections. Configured environment overrides remain excluded by default, with deliberate entry selection and secret-free confirmation distinct from argument handling. Suspected credential arguments require visible user handling before persistence, not silent deletion claimed as an exact copy. Preserve unrelated file content/comments, conflict safety, complete-candidate acceptance and installed/portable isolation (R2/R5–R7/R9).

### Frozen foundation tickets and source precedence

T0001 and T0002 are unchanged at the owner's explicit request: their IDs, filenames, contents and **Todo** statuses remain untouched. Their earlier source references, including T0001's old R6/E10 capture seam and initial-directory facts, are historical; freezing files does not establish completion or verify implementation. Current spec 0052 and ADR 0110 govern expanded capture/relaunch, not the superseded capture fallback in ADR 0108 or the running-only/fresh-session/virtual-environment scope in ADR 0109. ADR 0106's separate local named-launch directory precedence and ADR 0107's UI baseline remain applicable; ADR 0108 retains safe-save protections.

All expanded capture, recognized-agent lifetime, schema, validation, coherent publication and environment-aware Host relaunch requirements belong to **T0003**, including changes needed in production seams originally touched by T0001/T0002. They are not silently reassigned to the frozen ticket files. Initial launch provenance may be retained, but is not evidence of current cwd for Save. Local named-launch precedence (explicit > configured > usable local OSC 7 > Harbor process directory) remains distinct from capture (R3/R6).

## Delivery Surfaces

| Surface | Delivery ownership and boundary |
| --- | --- |
| Configuration schema/serialization/validation and accepted future policy: `crates/harbor-config/src/lib.rs`, reload/factories/catalog | T0001 baseline; T0003 owns extensions for environment/project/agent data under the same complete-candidate contract (R1/R2/R6/R7/R9). WSL/SSH directories are not native-local paths; validation authorizes no implicit network probes. |
| Source provenance, guided integration, current-directory recognition and scoped last-agent record; `src/profile_save.rs` and session/Host seams | T0003 owns stable capture/correction, retained original argv and recognition beyond agent exit (R6/R9). No concrete integration protocol, observation mechanism or module layout is selected here. |
| Commands, palette, popup/editor lifetimes and new-session menu; `crates/harbor-app/src/quick_save.rs` and existing command/palette/popup/tab-manager/workspace surfaces | T0002 chooser baseline; T0003 naming/review/correction/confirmation and its expanded data. Preserve shared interaction and focus/IME protections (R4/R5). |
| Host/factory/platform PTY launch; `src/profile_launch.rs`, `src/shell.rs`, `src/tab_coordinator.rs`, `crates/harbor-pty/src/windows.rs` | T0001 native-local baseline; T0003 owns environment → directory → selected agent relaunch and original argument meaning across boundaries (R1–R3/R9). No UI shell-string executor. |
| Selected-file writer, accepted catalog and watcher/manual coordination; `src/profile_save.rs`, `src/shell.rs` and config writer | T0003 safe durable save plus coherent publication, not only UI or serialization (R2/R7). |
| Focused contract tests and integrated Windows evidence/user/status/protocol docs | Each delivery ticket supplies focused verification; T0004 owns combined E1–E19 evidence, quality-gate records and truthful documentation (R8). |

Paths identify existing seams, not mandated new types/files or proof of delivered support. In particular, the inspected local-metadata/initial-directory capture is not accepted R6 behavior (spec Solution and Integration Seams).

## Dependencies

| Producer | Consumer | Why the consumer cannot complete first |
| --- | --- | --- |
| T0001 | T0002 | The chooser needs the validated accepted catalog and typed Host launch foundation with local directory/failure policy. |
| T0001 | T0003 | Quick save extends the profile schema/validation/publication and retained source-provenance foundations; T0003 supplies all new environment-aware semantics, not an initial-cwd capture fallback. |
| T0002 | T0004 | Combined native evidence needs the actual shared chooser, both entry paths and launch/focus interaction. |
| T0003 | T0004 | Combined native evidence needs the complete capture/review/persistence/publication/environment-agent relaunch delivery. |

T0001 is a foundation with two consumers and independently observable child/configuration outcomes, not a speculative refactoring ticket. T0002 and T0003 remain parallel after T0001: T0003 can verify accepted catalog publication and the Host environment/agent launch contract independently of the final chooser. T0004 validates the combined chooser interaction. Shared files alone add no blocker. The graph is acyclic: T0001 → {T0002, T0003} → T0004.

## Coordination

| Tickets | Risk | Strategy |
| --- | --- | --- |
| T0002, T0003 | Shared command/palette/popup/workspace/Host files, catalog updates and stale selection/input handling | Coordinate shared-file edits and compatible flow/lifetime integration. Use the existing typed command and exclusive-popup boundaries; no competing controller, second command bus or general popup stack. T0003's expanded accepted data must remain usable through the shared catalog. |
| T0001, T0003 | Baseline schema, native-local validation and provenance are insufficient for expanded save/relaunch | Preserve baseline profile/legacy compatibility while T0003 extends schema, complete-candidate validation/publication and Host launch boundaries. Do not interpret WSL/SSH directories as native-local or use launch fallback as capture truth. Frozen ticket files do not freeze production seams. |
| All | Existing-child stability, original argv, native paste confirmation, portable isolation and confidentiality | Keep regression obligations in delivery tests and T0004's combined evidence. Save confirmation alone is internal; automatic config writes remain unauthorized. |

## Recommended Order

1. Establish and verify T0001's baseline launch/configuration/provenance contract; its Todo status is not an acceptance claim.
2. Deliver T0002 and T0003 in parallel with coordination, or sequentially for shared-file convenience. Keep T0003's tightly coupled capture-to-relaunch behavior together; no new plan artifact or pre-refactoring ticket is required by this handoff.
3. T0004 integrates both flows, records native evidence and documents only implemented/evidenced behavior.

Focused tests belong in the delivery slices, not solely in T0004. At execution start, obtain separate authorization to create/link implementation sub-issues under #157; local tickets do not authorize remote mutation (R8).

## Requirement Coverage

| Contract | Primary delivery allocation | Integrated verification |
| --- | --- | --- |
| R1; E1/E2/E3 | T0001 independent profiles/legacy/argv baseline; T0002 selection; T0003 preserves compatibility across its extensions | T0004 actual child inputs, both entry paths and visible failures |
| R2; E3/E5/E6/E14 | T0001 baseline validation/reload; T0002 stale selection; T0003 owns all expanded schema/validation/publication and environment-directory interpretation | T0004 rejection, coherent catalog/factories, isolation and old-child stability |
| R3; E4/E5 | T0001 local directory precedence; T0003 preserves its separation from save capture and explicit WSL/SSH relaunch | T0004 local URI/fallback/no-network checks, never treating fallback as Save cwd |
| R4; E1/E6/E7 | T0002 shared chooser; T0003 publishes expanded accepted presets to that contract | T0004 combined chooser/reload/lifetime/focus interaction |
| R5; E8/E9/E10/E18 | T0003 input-only naming, correction/review, internal confirmation, credential handling and input lifetime | T0004 button/Enter/IME/return/cancel/no-new-window evidence |
| R6; E10/E11/E15/E16/E19 | T0003 environment/current-directory capture, stable draft, scoped agent information and configured-override opt-in | T0004 supported shells, changed/unknown cwd, scope isolation and exclusions |
| R7; E12/E13/E14 | T0003 selected-file safe persistence and accepted publication | T0004 save/chooser/restart/relaunch, conflicts/faults and mode isolation |
| R8; E1–E19 | Delivery tickets supply focused results; T0004 owns integrated evidence, quality gates, docs and package disposition | T0004 retrievable scoped results and explicit omissions |
| R9; E15/E16/E17/E18/E19 | T0003 last supported invocation lifetime, original argv/resume, environment/project/agent relaunch and visible suspected-secret handling | T0004 all four agents, supported environments, exited-agent and argument/scope checks |

## Ticket Index

| Ticket | File | Outcome |
| --- | --- | --- |
| T0001 | [T0001-profile-launch-contract.md](./T0001-profile-launch-contract.md) | Unchanged Todo foundation: independent profiles, native-local launch/failure policy, coherent acceptance and source launch facts |
| T0002 | [T0002-two-level-profile-chooser.md](./T0002-two-level-profile-chooser.md) | Unchanged Todo chooser: both entry paths, shared second-level palette and Host invocation |
| T0003 | [T0003-confirmed-quick-save.md](./T0003-confirmed-quick-save.md) | Environment-aware capture → correction/review → safe persist/accepted catalog → environment/project/agent relaunch, including needed schema/Host extensions |
| T0004 | [T0004-windows-evidence-and-documentation.md](./T0004-windows-evidence-and-documentation.md) | Integrated E1–E19 Windows evidence, quality-gate records and truthful user/package documentation |

## Handoff Limits

This update consumes settled spec/record requirements and the owner's allocation instructions; it establishes no new domain facts, architecture or acceptance policy. Integration/capture protocol, installation procedure, schema spelling, incidental APIs and write/recognition algorithms remain unselected implementation details within the source guarantees. Return material semantic gaps to requirements refinement rather than inventing assumptions.

Multi-layer nesting and all virtual-environment detection/activation (venv/Conda/Poetry) are deferred. No arbitrary command/history replay, arbitrary wrapper/alias or pre-integration history recovery promise, unsolicited local/remote configuration changes or remote installation, credential persistence/perfect detection claim, runtime environment dump or process cloning is authorized (R6/R9/E19). Configured-override opt-in remains separate and unchanged.

No implementation verification, runtime QA, completion/status promotion, spec/ADR/context change, additional ticket, plan, Git-policy change, staging or commit is part of this revision. T0004's evidence obligations are future work, not executed results.

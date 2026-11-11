# Shared Two-Level Profile Chooser

**Ticket ID:** T0002
**Source:** [Spec 0052 R4 and relevant R1–R3; E1/E6/E7](../../spec/0052-named-session-profiles-and-quick-save.md); [ADR 0107](../../adr/0107-shared-palette-profile-flow-and-in-window-confirmation.md), [spec 0033](../../spec/0033-exclusive-extensible-application-popups.md).
**Status:** Todo

## Goal

A user reaches profile selection through the new-session entry or command palette, chooses a named recipe in a second-level palette view, and gets exactly one correctly launched/focused terminal.

## Affected Surfaces

Application commands/catalog, palette presentation/filtering/navigation, popup/editor lifetimes, new-session entry composition and Host action admission/execution. Use T0001's accepted profile policy.

## Direction and Boundaries

Reuse the existing command palette with a second-level chooser, meaningful title, return control and filtering field, rather than an independent profile panel. Navigate within one active flow and retain existing exclusive-popup cleanup. Shared widgets remain generic; actual profile launch executes only through the typed command/Host path.

## Dependencies and Coordination

- **Blocked by:** T0001's validated catalog and real Host launch contract.
- **Blocks:** T0004's combined entry-path/native-focus evidence.
- **Parallel with:** T0003 after T0001; neither needs the other's UI to verify its own contract.
- **Coordination:** Both touch palette/commands/popup/Host integration. Agree on compatible lifecycle extension and shared-file ownership; do not create a second command executor or popup stack.

## Acceptance

- [ ] New-session profile selection and the palette's profile-opening action reach the same chooser/catalog. Ordinary New tab/Ctrl+T remain unchanged.
- [ ] Second-level title, return control, query filtering and valid selection work with mouse and keyboard using existing palette conventions; no-match/empty state cannot create a phantom session.
- [ ] Invoking an available profile creates one child through T0001's launch policy and focuses the resulting terminal; failure is observable, not inferred success from queue insertion.
- [ ] Accepted profile reloads update available choices coherently; rejected reloads retain old accepted choices. Revalidate edited/deleted/stale selections without launching an obsolete recipe or unrelated target.
- [ ] Search/palette replacement keeps one participating popup, retires old input lifetimes and does not resurrect displaced search or old query/results.
- [ ] Consumed opening/filter/navigation/selection/back/close/IME events do not reach the PTY; stale queue/clipboard/IME delivery cannot execute or edit a replacement flow.
- [ ] Existing palette command availability, disabled-without-reason presentation, editor semantics and native paste-confirmation gate remain intact.
- [ ] Focused real action-queue/composition tests prove execution counts, current-state checks and destination focus. Supply scoped native outcomes/omissions for T0004.

## Out of Scope

Saving/writing presets, new palette matching/ranking algorithms, a full profile editor, a generic popup stack, or separate UI-to-PTY launch logic.

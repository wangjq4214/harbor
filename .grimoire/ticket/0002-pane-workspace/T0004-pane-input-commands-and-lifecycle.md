# Integrate Pane Commands, Input Safety and Exit Lifecycle

**Ticket ID:** T0004
**Source:** [Spec 0056](../../spec/0056-pane-workspace.md), R6–R7; command/attachment integration of R1–R5; E1–E12, especially E4/E7–E10.
**Status:** Todo

## Goal

Make the split workspace usable through actual application commands while guaranteeing correct pane/session input, confirmation lifetimes, close/collapse and natural-exit behavior.

## Affected Surfaces

- Typed command registry, configurable defaults, dispatcher, workspace/tab UI and Host command execution.
- Focus/hit testing/pointer capture, terminal-local coordinates and IME.
- User paste and OSC 52 confirmations, search/results, profile capture and metadata consumers.
- Session exit/output/launch/resize races and full session-resource retirement.

## Approach

Connect the identity-qualified inherited/fallback launch path and pane layout through the existing application command path. Use one authoritative pane/session target per operation and revalidate delayed results. Do not register placeholder or partially safe production split commands.

Keep pointer hit/capture routing distinct from keyboard focus, and keep all visible panes distinct from the single intended keyboard/IME target. Extend existing source cancellation/authorization checks to pane changes without weakening the cross-window paste gate.

Retain the meaning of the existing close-tab command ID and rail close action. Change its default chord to `Ctrl+Shift+W`; provide focused-pane close on `Ctrl+W`. New command spelling and other split/focus/maximize chords remain implementation details under the established configurable registry contract.

## Dependencies and Coordination

- **Blocked by:** T0002 for required production launch/fallback behavior; T0003 for mounted geometry/visibility and split-tree interaction.
- **Blocks:** T0005's complete integrated runtime acceptance.
- **Coordination risks:** Confirmations, search, source metadata, configuration and close events must all use the same live session bindings established in T0001. Launch completion and shell exit must distinguish startup failure from later natural exit.

## Acceptance

- [ ] Horizontal/vertical split, pane focus by click/shortcut, ratio dragging and maximize/restore operate through the intended application/widget boundaries and focus the correct resulting session.
- [ ] Split results use the operation's intended source/workspace, including delayed inherited failure/fallback; switching or closing cannot attach a late result to another workspace.
- [ ] Default `Ctrl+W` closes the focused pane; default `Ctrl+Shift+W` and rail close close the entire tab. Explicit configured close-tab bindings retain whole-tab semantics.
- [ ] Overrides, unbinding, duplicate/conflicting binding rejection and consumed/pass-through behavior match existing contracts. Consumed UI chords execute once and write no PTY bytes.
- [ ] Keyboard, IME/preedit/candidate positioning, selection/copy/paste, mouse reporting, wheel, scrollbar and pointer capture route to the correct pane/session using its local geometry; divider drag does not leak terminal input.
- [ ] Focus loss, unmount, closure and lifecycle cancellation retire capture/transient IME state without leaking late composition or clipboard delivery.
- [ ] Paste confirmation preserves the existing independent-window gate and adds explicit original-session lifetime protection: the current completion path receives the then-active terminal, so retaining/revalidating the original target is new work. Pending paste/OSC 52 requests stay source-bound; unfocused visible panes cannot obtain foreground-write permission, and natural exit or other retirement cannot redirect a pending request to a survivor/replacement.
- [ ] Search/highlights/navigation, profile capture and source metadata preserve their existing session/provenance semantics without adding cross-pane search or new shell workflows.
- [ ] Shell-session exit automatically closes its pane and collapses the layout; ordinary foreground-program completion does not. Explicit close and exit races retire the session only once.
- [ ] A surviving pane receives deterministic eligible focus without process/content loss. Last-pane close closes its tab; last-tab close closes the window without replacement. Hiding and switching never terminate sessions.
- [ ] Late output/exit/ACK/search/launch results after retirement cannot target survivors, revive registrations or leave child/reader/PTY/bridge resources orphaned.
- [ ] Mounted application routing and focused Host/PTY tests cover several simultaneous producers, distinct typed markers, IME/capture cancellation, confirmation races and close/fallback/resize races. Do not substitute unit tests for T0005's real Windows matrix.

## Out of Scope

Cross-window pane movement, persisted workspaces, new clipboard permission modes, global search, arbitrary process replay and unrelated profile-save redesign.

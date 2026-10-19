# Usable Palette for Tabs and Configuration Reload

**Ticket ID:** T0001
**Source:** [Spec 0030](../../spec/0030-command-palette.md), R1–R7 for the tabs/reload increment; [ADR 0069](../../adr/0069-command-palette-source-aware-availability-and-focus.md); [relationship and coordination contract](./README.md).
**Status:** Todo

## Goal

A user opens the palette, finds an existing tab/reload action by label or ID, sees current shortcuts, invokes it once through the existing Host path, and reaches the correct terminal/window outcome without leaking consumed input. This is a usable partial delivery, not the full visible command catalog or #156 completion.

## Affected Surfaces

- Application registry/keybindings and workspace/palette composition; existing application action inbox.
- Host command reduction, existing tab execution and configuration reload/publication.
- Reusable editor/focus/Portal integration and focused registry/configuration/routing/Host tests. Only demonstrated generic routing issues may require widget changes.

## Approach

Keep common discovery, editing, selection, availability/revalidation, feedback and the first real consumers in one vertical slice. Reuse registry metadata and effective bindings rather than copy catalogs/default chords. Keep Host effects and result-aware focus; do not move application policy into widgets or bypass the typed FIFO. Do not freeze module layout or signatures.

T0001's invokable command subset is existing palette-visible reload and tab commands. Document remaining terminal/search commands as not yet integrated until T0002; do not fake them as working or change their registry visibility merely to claim completion. Preserve existing operation semantics, including final-tab window close and numeric-tab bounds.

## Dependencies and Coordination

- **Blocked by:** None within this set; inspect the current code and existing contracts before execution.
- **Blocks:** T0002's common palette consumer path; T0003's final integrated acceptance.
- **Coordination risks:** Workspace/command/Host files overlap with T0002. Deliver one reusable application path and carry actual contracts forward; no separate global pre-refactor or second editor.

## Acceptance

- [ ] Default Ctrl+Shift+P opens/focuses the query; its registered binding supports existing override, multiple binding, unbinding and conflict/fallback/rejection behavior. Document the chosen stable ID.
- [ ] Empty query exposes the delivered subset; mixed-case label/ID substring filtering and no-match presentation work. Up/Down/Enter maintain a valid selection after filtering and cannot execute a phantom row.
- [ ] Command rows display effective chords rather than defaults. Accepted reload updates the source; rejected reload retains accepted bindings and display.
- [ ] The query uses the reusable single-line editor. Editing/copy/cut/paste/undo affect only query state; preedit is transient, candidate confirmation does not execute, composition-first Escape is preserved.
- [ ] Tabs/reload execute once through the existing queue/drain path. Currently unavailable candidates show reasons and are rechecked; rejection never forwards palette input to the terminal. Actual errors/rejections are distinguishable from execution, with reload errors retained.
- [ ] Cancel/close restores terminal focus and IME eligibility. New/activated/remaining tabs receive the correct terminal destination; final-tab close preserves existing window-close behavior. Reload does not steal a command-specific destination.
- [ ] Closed/replaced editor state cannot accept stale clipboard/IME edits; existing focus-loss/root-lifecycle contracts remain intact.
- [ ] Focused tests count queued/executed operations and capture PTY writes for opener, editing, navigation, Enter/Escape and associated key-up/IME transitions. Terminal pass-through resumes after close. Cover spec E1–E3, E5–E7 and E9 for this subset; native evidence is finalized in T0003, not inferred from headless tests.
- [ ] Record delivered subset and focused reproducible checks without claiming all R1 or #156 is complete; retain T0002/T0003 obligations.

## Out of Scope

Terminal copy/paste/scroll/search command integration belongs to T0002; integrated Windows acceptance and final status documentation belong to T0003. No profiles, panes, appearance feature implementation, fuzzy matching, second command bus or new palette editor.

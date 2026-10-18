# Complete Existing Terminal Actions and Destination Focus

**Ticket ID:** T0002
**Source:** [Spec 0030](../../spec/0030-command-palette.md), remaining R1 and terminal-context R3–R7; [ADR 0069](../../adr/0069-command-palette-source-aware-availability-and-focus.md); [relationship and coordination contract](./README.md).
**Status:** Todo

## Goal

Complete the first-delivery catalog with existing copy/paste, primary-buffer scrolling and current-session search commands. Palette invocation honors context, clipboard/confirmation policy and the resulting focus destination without forwarding consumed input or changing shortcut pass-through semantics.

## Affected Surfaces

- T0001's application palette/common command availability and invocation path.
- Existing Host terminal-command execution, search/tab coordination, clipboard/confirmation and focus handling.
- Focused application/routing/Host integration tests with PTY writer capture and stale-context/late-result cases.

## Approach

Extend the delivered common path, not a second terminal-only palette executor. All currently implemented `palette_visible` registry entries must be covered when this ticket finishes. Preserve palette-hidden copy-or-interrupt and internal-only intents. Reuse existing operation semantics and source-specific rejection versus shortcut pass-through under ADR 0069.

Availability presentation and invocation must use the same current facts, but disabled palette actions cannot forward Enter. Respect search/confirmation focus destinations after palette teardown; an unconditional terminal-refocus step is not acceptable. Keep existing clipboard authorization and cross-window gates.

## Dependencies and Coordination

- **Blocked by:** T0001; a real mounted palette and common action path are required for integration and verification.
- **Blocks:** T0003's complete first-delivery native acceptance.
- **Coordination risks:** Shared command/workspace/Host surfaces; integrate with T0001's actual implementation rather than duplicating metadata/state. Future feature owners are not blockers.

## Acceptance

- [ ] Existing visible copy/paste, page/top/bottom scroll and search open/close/next/previous commands are discoverable, show effective bindings and use the existing Host path exactly once.
- [ ] Copy uses always-copy semantics, including empty copy without selection. Copy-or-interrupt remains hidden; query editing copy/paste does not invoke terminal commands.
- [ ] Alternate-screen scroll actions are visibly disabled with reasons; changed availability is rechecked before execution. Disabled/rejected actions inject no Enter, navigation bytes or synthetic markers into PTY.
- [ ] Ordinary terminal Ctrl+C/no-selection and alternate-screen scroll pass-through still behave as before after palette close; palette safety does not globally disable terminal shortcuts.
- [ ] Search open targets its reusable editor, search close returns to terminal, match navigation preserves existing semantics. Palette closure does not override the action's destination or search query policy.
- [ ] Paste retains existing clipboard, bracketed-paste and multiline confirmation behavior; the existing separate-window input gate applies, including attempts to invoke commands while confirmation is active. Late clipboard/IME results cannot mutate the closed palette or another target.
- [ ] Execution feedback distinguishes rejection/failure from accepted execution, using actual outcomes rather than treating queue insertion as success. No existing operation error/confirmation is suppressed.
- [ ] Context changes, tab/session removal, window focus loss and supported root replacement do not execute a stale selection or deliver stale editor input. Do not introduce a new persistence or target-capture policy contrary to existing command semantics.
- [ ] Focused real-path tests cover spec E4–E9, including once-only queue/drain execution, query-versus-terminal editing, IME/close transitions, destination focus and PTY byte capture. Native evidence remains a T0003 requirement.
- [ ] Compare the delivered catalog against the inspected current registry: all implemented visible entries covered, hidden/internal entries absent, future profiles/panes/appearance still explicit deferrals.

## Out of Scope

Implementing profiles, panes or application appearance switching; changing underlying search/clipboard/terminal behavior; new command bus/editor; declaring integrated Windows acceptance from model tests alone.

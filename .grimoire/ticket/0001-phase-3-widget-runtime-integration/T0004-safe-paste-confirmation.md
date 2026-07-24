# Safe Paste Confirmation

**Ticket ID:** T0004
**Source:** [Spec: 0001-phase-3-widget-runtime-integration](../../spec/0001-phase-3-widget-runtime-integration.md)
**Status:** Todo

## Goal

The native Widget confirmation window sends unchanged raw content only after explicit confirmation, using terminal input modes read at confirmation time.

## Layers

- [ ] **`harbor-text`:** Reuse existing shared text runs for the Paste label; no new font/raster behavior.
- [ ] **`harbor-widget`:** Add the Paste control and event intents for click, `y`, and focused Enter while preserving deferred event effects.
- [ ] **`harbor-render`:** None — paste disposition remains the existing source of raw confirmation candidates.
- [ ] **App host:** Consume confirmation intent, reread terminal `InputModes`, call the existing raw `send_paste` path, and dispose of the confirmation window.
- [ ] **Verification:** Test bracketed-paste gating plus confirm/cancel behavior with changed modes and unchanged raw content.

## Approach

1. Extend the confirmation root with a Paste control and its focus/keyboard routes.
2. Keep raw content App-owned and ensure the Widget emits intent rather than owning PTY or input-mode state.
3. On intent, have App reread current terminal modes before forwarding the unchanged content through the established paste sender.
4. Retain every cancellation route from T0003 and ensure duplicate requests remain blocked until disposal.
5. Add end-to-end tests around disposition, mode changes, and observed PTY writes.

## Blocked by

- T0001 — Uses shared text labels.
- T0003 — Requires the native window and App-level gate.

## Blocks

- T0005 — Preview completion extends this functional confirmation UI.

## Acceptance

- [ ] Paste click, `y`, and focused Enter send exactly the original candidate content.
- [ ] The send uses `InputModes` read when confirmation occurs, not when the dialog opened.
- [ ] Cancel routes, bracketed paste, and non-confirmable paste never cause unintended PTY writes.
- [ ] The confirmation window closes after a terminal decision and the App gate is released.

## Out of Scope

- Preview scrolling and detailed preview layout.
- General command/callback infrastructure beyond the confirmation intents.
- Changes to terminal paste-disposition rules.

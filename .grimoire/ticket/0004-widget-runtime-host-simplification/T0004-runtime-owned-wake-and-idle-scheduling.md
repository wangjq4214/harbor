# Runtime-Owned Wake and Idle Scheduling

**Ticket ID:** T0004
**Source:** [Spec: 0004-widget-runtime-host-simplification](../../spec/0004-widget-runtime-host-simplification.md)
**Status:** Todo

## Goal

PTY output and Widget invalidation request only the necessary frames, and an idle Harbor process returns to `Wait` without App-owned scheduling policy.

## Layers

- [ ] **Runtime Host:** Convert `TerminalOutputReady` to generic external invalidation, apply redraw/control-flow effects, and reduce `user_event` and `about_to_wait` to thin forwarding code.
- [ ] **Winit Runtime Integration:** Own the frame scheduler and translate scheduler state into `RequestRedraw`, `Wait`, `WaitUntil`, or `Poll` effects.
- [ ] **Core Widget Runtime:** Connect dirty Fibers, event invalidation, animation deadlines, external invalidation, and frame completion to scheduling state.
- [ ] **Terminal / Application Components:** Keep PTY wake production and terminal output ownership unchanged; expose only the existing Host wake signal.
- [ ] **Verification:** Port scheduler state-machine tests and add an integration test covering PTY wake, coalesced redraw, frame completion, and idle control flow.

## Approach

1. Move `FrameScheduler`, redraw reasons, deadlines, and control-flow calculation from `src/event.rs` into the runtime integration/core boundary according to platform dependence.
2. Define external invalidation as generic runtime work rather than teaching `harbor-widget` about `AppEvent` or PTY output.
3. Merge repeated invalidations before `RedrawRequested` into one redraw effect while retaining reasons needed for diagnostics.
4. Have frame start/completion update scheduler state so active animation can Poll and steady state can Wait or WaitUntil.
5. Make App apply returned effects to `Window::request_redraw` and `ActiveEventLoop::set_control_flow` without calculating policy.
6. Remove obsolete App scheduler fields and helpers after state-transition parity is verified.

## Blocked by

- T0001 — provides external invalidation and RuntimeEffects contracts.
- T0003 — must finish overlapping main-window event branch changes first.

## Blocks

- T0005 — frame presentation consumes redraw and frame-completion state.

## Acceptance

- [ ] One `TerminalOutputReady` wake produces a redraw request and updates terminal output on the next frame.
- [ ] Multiple wakes before redraw coalesce into one window redraw request.
- [ ] Dirty Widget state and animation deadlines produce the expected redraw/control-flow effects.
- [ ] With no pending work, `about_to_wait` applies `Wait` and requests no redraw.
- [ ] Runtime does not expose `AppEvent`, PTY, or terminal-specific redraw reasons.
- [ ] `src/event.rs` no longer owns frame scheduling policy; `App` only forwards invalidation and applies effects.

## Out of Scope

- Changing Terminal's synchronous PTY reader architecture.
- GPU texture acquisition or presentation.
- Surface error recovery.
- Confirmation-window migration.

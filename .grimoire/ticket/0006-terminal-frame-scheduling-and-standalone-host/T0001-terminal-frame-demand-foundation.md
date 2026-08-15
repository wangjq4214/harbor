# Terminal Frame Demand Foundation

**Ticket ID:** T0001
**Source:** [Spec: 0006-terminal-frame-scheduling-and-standalone-host](../../spec/0006-terminal-frame-scheduling-and-standalone-host.md)
**Status:** Todo

## Goal

Establish a tested, host-neutral Terminal Frame Demand that reports immediate redraw work and the next cursor-blink deadline without scheduling a window itself.

## Layers

- [ ] **harbor-terminal state and rendering:** Add the Frame Demand contract and derive it from cursor visibility, blink state, input, and cursor-position changes; preserve render-pass-only drawing.
- [ ] **harbor-widget Runtime / CustomPaint:** None — this ticket defines the terminal-side contract that the widget slice will consume.
- [ ] **Runtime Host / winit-wgpu adapter:** None — host-specific deadline application belongs to T0002 and T0003.
- [ ] **Verification:** Add deterministic terminal tests for enabled/disabled blinking, next-deadline calculation, immediate invalidation, and reset-to-visible behavior.

## Approach

1. Define a public, host-neutral frame-demand value in `harbor-terminal` that can express immediate redraw need and an optional earliest deadline.
2. Make cursor blink state calculate its next phase boundary and expose that state through Terminal without introducing winit, surface, queue, or presentation ownership.
3. Ensure terminal input and cursor movement reset the cursor to visible and produce a demand suitable for an immediate host frame.
4. Make time-sensitive behavior testable with controlled instants or an equivalent deterministic seam; retain existing render-time cursor preparation as the consumer of the same state.

## Blocked by

- None.

## Blocks

- T0002 — widget scheduling consumes Frame Demand.
- T0003 — the standalone host consumes Frame Demand.

## Acceptance

- [ ] A blinking visible cursor reports its next phase deadline without any host polling API.
- [ ] A steady or terminal-hidden cursor reports no blink deadline.
- [ ] Input and cursor-position changes make the cursor visible immediately and restart its deadline sequence.
- [ ] Terminal retains no winit, Window, Surface, Queue, Device, submission, or presentation ownership.
- [ ] Focused terminal tests cover phase transitions, reset behavior, and deadline absence when blinking is disabled.

## Out of Scope

- Widget registration, Runtime deadline merging, and winit event-loop changes.
- A standalone window, surface, or GPU host.
- Cursor style/protocol changes and configurable blink intervals.

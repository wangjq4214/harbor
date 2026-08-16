# Standalone Terminal Winit/WGPU Host

**Ticket ID:** T0003
**Source:** [Spec: 0006-terminal-frame-scheduling-and-standalone-host](../../spec/0006-terminal-frame-scheduling-and-standalone-host.md)
**Status:** Todo

## Goal

A public feature-gated companion host renders Terminal directly through winit and wgpu, with the same idle blink, reset, and drawable-surface behavior as widget hosting.

## Layers

- [ ] **harbor-terminal state and rendering:** Consume T0001's Frame Demand and render Terminal into the companion host's supplied wgpu render pass; do not add `winit` ownership or dependencies to the core engine.
- [ ] **harbor-widget Runtime / CustomPaint:** None — this independently demonstrable path intentionally creates no Runtime or CustomPaint.
- [ ] **Runtime Host / winit-wgpu adapter:** Add the feature-gated companion adapter, public construction/event-loop API, deadline-to-control-flow handling, render-pass lifecycle, and non-drawable surface suspension/recovery.
- [ ] **Verification:** Add adapter-level tests for effects/control flow and an executable or integration path proving direct idle blink, reset, and surface recovery without `harbor-widget`.

## Approach

1. Introduce the direct host at a companion adapter boundary rather than adding `winit` to the core `harbor-terminal` crate, preserving ADR 0012.
2. Make the adapter own its Window/Surface/Device/Queue lifecycle and translate Terminal Frame Demand into `RequestRedraw` and `WaitUntil` decisions.
3. On redraw, acquire and configure the host surface, pass the active wgpu render pass to Terminal, submit and present; recover or suspend according to drawable-surface state.
4. Route direct terminal input through existing Terminal boundary types so input and cursor moves reset the shared blink state without widget involvement.
5. Add deterministic adapter tests and a direct-host integration demonstration that exercises idle, reset, and restoration behavior.

## Blocked by

- T0001 — provides the Frame Demand contract and reset semantics.

## Blocks

- None.

## Acceptance

- [ ] A consumer can opt into a public direct winit/wgpu terminal host without constructing `harbor-widget::Runtime` or `CustomPaint`.
- [ ] An otherwise idle blinking cursor causes discrete direct-host redraws and alternating visible/hidden phases.
- [ ] Direct input or cursor movement resets a hidden cursor to visible immediately and restarts the shared deadline sequence.
- [ ] Zero-sized, minimized, or unavailable surfaces suppress blink wakes; restoring a drawable surface requests one frame and resumes scheduling.
- [ ] The core Terminal remains free of winit, Window, Surface, Queue, Device, submission, and presentation ownership.

## Out of Scope

- Altering widget Runtime behavior; covered by T0002.
- New non-winit platform adapters, new terminal rendering backends, or a second blink implementation.
- Moving Runtime Host ownership into Terminal core.

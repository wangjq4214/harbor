# Host-Neutral Terminal Frame Scheduling

**Status:** Implementing
**Date:** 2026-09-11

## Context

Terminal cursor blinking needs deadline-driven redraws when no input or terminal output arrives. Harbor must embed Terminal through `CustomPaint` under the widget runtime while also supporting direct winit and wgpu rendering without a widget runtime; alternatives were terminal-owned scheduling or separate blink implementations for each host.

## Decision

Terminal exposes a host-neutral frame demand containing immediate invalidation and its next cursor-blink deadline. In widget mode, the `Runtime Frame Scheduler` exclusively applies that demand and owns window redraw, wake, surface acquisition, submission, and presentation; in direct mode, the standalone winit/wgpu host consumes the same demand and supplies the render pass without Terminal owning platform or GPU presentation resources.

## Consequences

- Cursor blinking schedules discrete deadline frames while idle instead of relying on continuous polling or unrelated input.
- Widget and standalone hosts share one terminal blink state machine and reset behavior.
- Widget-hosted Terminal rendering remains an external draw within the widget frame, so it cannot independently acquire, submit, or present a surface.
- Both hosting paths require regression tests for blink deadline transitions and input-triggered visibility reset.

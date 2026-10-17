# Terminal/Widget Boundary Migration

**Spec ID:** 0005
**Status:** Implemented
**Date:** 2026-08-15

## Requirement

Harbor must remove the `harbor-terminal` → `harbor-widget` dependency while preserving terminal behavior through a root-level bridge Component.

## Solution

`harbor-terminal` becomes widget-framework-independent while remaining a wgpu/winit terminal renderer for this phase. It owns a `RenderTarget` constructed from physical allocation and surface geometry, plus a `TerminalEvent` family containing the terminal's required keyboard, IME, pointer, and focus semantics. Its public rendering and input APIs accept only these terminal-owned types; no public terminal API exposes `ExternalDrawId`, `ExternalDrawContext`, `UiEvent`, or other `harbor-widget` types. The existing viewport model is refactored around the terminal-owned render target rather than duplicated.

The root crate adds `TerminalWidgetBridge: Component`. It owns the widget-facing external-draw identifier and composes `CustomPaint`; its render handler converts widget draw geometry into `RenderTarget` before calling `Terminal::render`, and its input adapter converts routed `UiEvent` values into `TerminalEvent` before calling `Terminal::handle_event`. It retains the existing frame-scoped GPU-access discipline required by the `'static` external-draw handler.

The App remains the Runtime Host: it constructs and owns the terminal, drains deferred external input, applies the cross-window input gate, and determines lifecycle and error policy. Once input is permitted and the external-draw identifier matches, the App delegates adaptation and delivery to `TerminalWidgetBridge`. The bridge does not move Host policy, PTY lifecycle, or window/GPU ownership into either crate.

## Decisions

The root bridge preserves behavior rather than widget-typed terminal signatures. App-owned policy cannot move into the bridge ([ADR 0009](../adr/0009-app-cross-window-input-gate.md)). The phase's retained winit dependency conflicts with ADR 0012's no-winit rule; later core extraction is governed by [Spec 0016](./0016-terminal-engine-wgpu-renderer-boundary.md), not resolved by this cleanup.

### Use a root-level bridge Component

The bridge belongs with the consumer of both crates, without a dedicated bridge crate or exposed `AnyView`.

## Test Plan

The widget-independent terminal must build/test without harbor-widget and preserve rendering, input, focus, cursor, PTY and scrollback semantics. Gate-active event drains cannot bypass paste safety.

- **Integration tests:** Cover `ExternalDrawContext`→`RenderTarget` and `UiEvent`→`TerminalEvent` mappings, identifier matching, allowed versus gated event forwarding, and terminal PTY/scrollback behavior after mapped input. Add a Cargo dependency/build assertion proving `harbor-terminal` does not depend on `harbor-widget`.
- **Manual tests:** Run an interactive shell; type normal and modified keys, IME text, wheel-scroll and scrollback keys; resize the window; open paste confirmation and verify gate behavior; verify cursor shape, visibility, and current blink behavior are unchanged.
- **Performance thresholds:** Preserve the existing sub-2 ms 80×24 terminal frame target; introduce no recurring redraw, extra full-scene rebuild, or additional terminal-to-widget dependency.
- **Edge cases:** Zero-size or clipped allocations; pointer events not recognized by terminal input; focus gain/loss; unmatched external-draw identifiers; event drain while the gate is active; layout/DPI changes; absent terminal renderer or unavailable frame-scoped GPU access.

## Out of Scope

- Cursor blink deadline scheduling, Runtime scheduler changes, and external-paint deadline providers.
- Splitting a fully UI-neutral `terminal-core` from a wgpu renderer or removing winit from the terminal crate.
- A new `harbor-terminal-widget` crate or a public reusable bridge API.
- Changing `CustomPaint` deferred-input semantics or adding a generic external input-handler registration mechanism.
- Altering parser behavior, PTY I/O strategy, terminal rendering behavior, widget event routing, GPU/window ownership, or cross-window policy.
- New GPU integration-test infrastructure.

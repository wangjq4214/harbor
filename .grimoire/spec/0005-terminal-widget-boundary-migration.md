# Terminal/Widget Boundary Migration

**Spec ID:** 0005
**Status:** Draft
**Date:** 2026-08-15

## Requirement

Harbor must remove the `harbor-terminal` → `harbor-widget` dependency while preserving terminal behavior through a root-level bridge Component.

## Solution

`harbor-terminal` becomes widget-framework-independent while remaining a wgpu/winit terminal renderer for this phase. It owns a `RenderTarget` constructed from physical allocation and surface geometry, plus a `TerminalEvent` family containing the terminal's required keyboard, IME, pointer, and focus semantics. Its public rendering and input APIs accept only these terminal-owned types; no public terminal API exposes `ExternalDrawId`, `ExternalDrawContext`, `UiEvent`, or other `harbor-widget` types. The existing viewport model is refactored around the terminal-owned render target rather than duplicated.

The root crate adds `TerminalWidgetBridge: Component`. It owns the widget-facing external-draw identifier and composes `CustomPaint`; its render handler converts widget draw geometry into `RenderTarget` before calling `Terminal::render`, and its input adapter converts routed `UiEvent` values into `TerminalEvent` before calling `Terminal::handle_event`. It retains the existing frame-scoped GPU-access discipline required by the `'static` external-draw handler.

The App remains the Runtime Host: it constructs and owns the terminal, drains deferred external input, applies the cross-window input gate, and determines lifecycle and error policy. Once input is permitted and the external-draw identifier matches, the App delegates adaptation and delivery to `TerminalWidgetBridge`. The bridge does not move Host policy, PTY lifecycle, or window/GPU ownership into either crate.

### Seams

| Seam | Connects | Expects | Provides |
| --- | --- | --- | --- |
| Terminal boundary | `TerminalWidgetBridge` ↔ `harbor-terminal` | `RenderTarget`, `TerminalEvent`, and frame-scoped render access | Widget-independent terminal rendering and terminal input handling |
| External paint integration | `TerminalWidgetBridge` ↔ `harbor-widget` | `CustomPaint`, `ExternalDrawId`, `ExternalDrawContext`, deferred `UiEvent`, and external-draw callback registration | Widget paint-order/clipping integration and mapped terminal input |
| Host input policy | App ↔ `TerminalWidgetBridge` | Terminal lifecycle, drained external input, and cross-window gate result | Delivery only of permitted, identifier-matched terminal events |

## End-to-End Tests

### E2E: Terminal renders through the bridge

- **Given:** The App owns a running Terminal and sets a `TerminalWidgetBridge` as the main Runtime's terminal component.
- **When:** Terminal output invalidates the Runtime or layout changes the component's allocation.
- **Then:** `CustomPaint` invokes the bridge handler, the bridge supplies a terminal `RenderTarget`, and the terminal renders in the same widget allocation, paint order, clipping, and resize behavior as before.

### E2E: Permitted terminal input preserves semantics

- **Given:** The bridge's `CustomPaint` has focus and the cross-window input gate permits terminal input.
- **When:** The user types, composes IME text, uses pointer/wheel input, or invokes scrollback navigation.
- **Then:** Widget input is deferred and drained as before, the bridge maps it to `TerminalEvent`, and the terminal preserves existing PTY bytes, scrollback behavior, focus behavior, and redraw consequences.

### E2E: Cross-window gate remains authoritative

- **Given:** A paste-confirmation window is open while the main Runtime contains the terminal bridge.
- **When:** The user sends keyboard input, requests another paste, scrolls, or terminal output arrives.
- **Then:** The App continues to block keyboard input and new paste requests while allowing the existing permitted scrollback, output, and rendering behavior; the bridge does not bypass the gate.

### E2E: Terminal crate is widget-independent

- **Given:** The workspace builds `harbor-terminal` without `harbor-widget` as a dependency.
- **When:** The terminal crate is compiled and its tests run.
- **Then:** It exposes only terminal-owned render and input boundary types while preserving its parser, PTY, and wgpu/winit rendering functionality.

## Decisions

### Use terminal-owned render and input contracts

- **Choice:** Replace widget types in the terminal public surface and internals with `RenderTarget`, `TerminalEvent`, and their terminal-owned supporting types; retain wgpu and winit for this phase.
- **Reason:** This removes the actual crate dependency rather than merely wrapping its public methods, while preserving the current rendering scope. ADR 0012's statement that terminal must not depend on winit conflicts with the explicitly scoped 5.1 requirement and the existing dependency; this spec records the exception and defers UI-neutral core extraction.
- **ADR reference:** [0012-consolidate-render-ui-into-terminal](../adr/0012-consolidate-render-ui-into-terminal.md), [0011-terminal-custompaint-gpu-injection](../adr/0011-terminal-custompaint-gpu-injection.md)

### Use a root-level bridge Component

- **Choice:** Add `TerminalWidgetBridge: Component` in the root project, not in `harbor-terminal`, `harbor-widget`, or a new crate.
- **Reason:** The root is the existing owner of terminal lifecycle and the only current consumer that may depend on both crates. Composing `CustomPaint` preserves its provider-by-ID behavior without exposing `AnyView` or creating a premature reusable package.
- **ADR reference:** [0005-custom-paint-provider-by-id](../adr/0005-custom-paint-provider-by-id.md), [0011-terminal-custompaint-gpu-injection](../adr/0011-terminal-custompaint-gpu-injection.md)

### Keep App-owned policy outside the bridge

- **Choice:** The App retains deferred-input draining, cross-window gating, terminal lifecycle, and resource ownership; the bridge performs only widget-to-terminal adaptation.
- **Reason:** Cross-window safety belongs to the only owner of both window event streams and the PTY, and terminal/window/GPU resource lifetimes remain Host responsibilities.
- **ADR reference:** [0009-app-cross-window-input-gate](../adr/0009-app-cross-window-input-gate.md), [0015-runtime-owned-frame-presentation](../adr/0015-runtime-owned-frame-presentation.md)

### Preserve behavior, not obsolete widget-typed signatures

- **Choice:** Treat rendering, resize, input, scrollback, cursor, and PTY behavior as compatibility obligations while permitting source-incompatible terminal API signatures.
- **Reason:** The prior signatures themselves carry the dependency being removed; retaining them would violate the requirement. The external paint path continues to preserve widget layout, clipping, and paint order.
- **ADR reference:** [0005-custom-paint-provider-by-id](../adr/0005-custom-paint-provider-by-id.md), [0011-terminal-custompaint-gpu-injection](../adr/0011-terminal-custompaint-gpu-injection.md)

### Defer cursor deadline scheduling

- **Choice:** Do not add blink deadlines, scheduler changes, or external-paint deadline providers in this migration.
- **Reason:** The requirement is a dependency-boundary refactor; frame scheduling is separately owned by the Runtime integration and must not be changed as incidental migration work.
- **ADR reference:** [0015-runtime-owned-frame-presentation](../adr/0015-runtime-owned-frame-presentation.md)

## Test Plan

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

## Future Evolution

- Add external-paint deadline providers when the cursor blink fix is implemented without an App-specific scheduler path.
- Extract `terminal-core` and `terminal-wgpu` when true UI independence or an additional rendering host is required; resolve the ADR 0012 winit-dependency conflict at that time.
- Create a dedicated bridge crate only when a second application or reusable consumer needs the same Component.
- Generalize deferred external input handlers only when a bridge must autonomously receive widget input without App-mediated Host policy.

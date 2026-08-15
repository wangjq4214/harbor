# Terminal-owned Boundary Foundation

**Ticket ID:** T0001
**Source:** [Spec: 0005-terminal-widget-boundary-migration](../../spec/0005-terminal-widget-boundary-migration.md)
**Status:** Done

## Goal

Establish widget-free terminal render and input contracts that subsequent bridge slices can consume without changing observable application behavior.

## Layers

- [ ] **Terminal engine (`harbor-terminal`):** Add and export terminal-owned `RenderTarget`, `TerminalEvent`, and supporting key, modifier, pointer, and focus types; keep their definitions free of `harbor-widget` references.
- [ ] **Widget external-paint (`harbor-widget`):** None — this foundation intentionally preserves existing `CustomPaint` contracts until a bridge consumes the new terminal types.
- [ ] **Runtime Host / bridge (`src/`):** None — no Component or App wiring is introduced before the rendering slice.
- [ ] **Verification:** Add focused contract tests for boundary type construction, equality/translation-ready semantics, and compilation independent of widget types.

## Approach

1. Introduce a terminal boundary module in `crates/harbor-terminal/src/` and re-export only the types callers need from `lib.rs`.
2. Model render allocation and surface geometry once in `RenderTarget`, using the physical-coordinate data required by the existing viewport calculation.
3. Model only current terminal input semantics in `TerminalEvent` and its support types; do not carry `UiEvent` or widget implementation types through aliases or wrappers.
4. Add narrow terminal tests establishing the new contract's geometry and event representations while preserving the existing widget-typed APIs temporarily for later slices.

## Blocked by

- None — this is the pre-refactoring foundation.

## Blocks

- T0002 — terminal rendering needs `RenderTarget`.
- T0003 — terminal input migration needs `TerminalEvent` and its supporting types.

## Acceptance

- [ ] `harbor-terminal` exports terminal-owned render and input boundary types without importing `harbor-widget` in their definitions.
- [ ] `RenderTarget` represents the allocation origin, allocation size, and surface size needed by terminal viewport calculation.
- [ ] `TerminalEvent` represents all currently supported key, IME, pointer, and focus semantics without widget event types.
- [ ] Focused contract tests pass, and existing workspace behavior remains unchanged.

## Out of Scope

- Migrating `Terminal::render` or `Terminal::handle_event` implementations.
- Adding `TerminalWidgetBridge` or changing App wiring.
- Removing `harbor-widget` from `harbor-terminal/Cargo.toml`; that occurs only after all remaining terminal internals stop using widget input types.
- Scheduler, deadline, or cursor blink changes.

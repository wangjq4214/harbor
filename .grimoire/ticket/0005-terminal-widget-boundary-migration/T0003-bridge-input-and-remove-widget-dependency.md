# Bridge Input and Remove Widget Dependency

**Ticket ID:** T0003
**Source:** [Spec: 0005-terminal-widget-boundary-migration](../../spec/0005-terminal-widget-boundary-migration.md)
**Status:** Todo

## Goal

Permitted terminal input continues to produce the same terminal and PTY behavior through `TerminalWidgetBridge`, while `harbor-terminal` has no dependency on `harbor-widget`.

## Layers

- [ ] **Terminal engine (`harbor-terminal`):** Migrate `input.rs`, `io.rs`, and public input handling from `UiEvent` and related widget types to `TerminalEvent`; preserve encoder, scrollback, IME, focus, and PTY behavior; remove widget re-exports and the Cargo dependency.
- [ ] **Widget external-paint (`harbor-widget`):** No framework source change — retain `CustomPaint` deferred `UiEvent` queuing and Runtime input drain exactly as currently implemented.
- [ ] **Runtime Host / bridge (`src/`):** Map permitted, identifier-matched drained `UiEvent` values through `TerminalWidgetBridge`; keep App-owned event drain, terminal lifecycle, and cross-window keyboard/paste gate authoritative.
- [ ] **Verification:** Add mapper and routing tests, run terminal input/PTY tests, and verify the final Cargo dependency graph contains no `harbor-terminal` → `harbor-widget` edge.

## Approach

1. Replace terminal-internal widget event imports with the terminal-owned event family from T0001, including every currently supported keyboard, IME, pointer, wheel, and focus path.
2. Preserve terminal-side event encoding and scrollback semantics while changing only the event representation at its boundary.
3. Extend `TerminalWidgetBridge` with explicit `UiEvent` to `TerminalEvent` adaptation and a stable bridge draw-ID access point for App routing.
4. Update `src/app.rs` to drain the existing deferred external input, apply the existing cross-window gate and identifier match, then delegate the permitted event to the bridge.
5. Remove all terminal widget type exposure and `harbor-widget` dependency declarations; use compilation and dependency-graph checks to prevent reintroduction.

## Blocked by

- T0001 — supplies `TerminalEvent` and supporting terminal input types.
- T0002 — supplies `TerminalWidgetBridge` and its bridge-owned external draw identifier.

## Blocks

- None.

## Acceptance

- [ ] Keyboard, modified-key, IME, pointer/wheel, focus, and scrollback input preserve their existing terminal-visible and PTY-visible behavior when the gate permits delivery.
- [ ] While paste confirmation is open, App-owned keyboard/paste restrictions remain effective and the bridge cannot bypass them; existing permitted output/rendering/scrollback behavior remains available.
- [ ] `harbor-terminal` has no source imports, public re-exports, or Cargo dependency on `harbor-widget`.
- [ ] Terminal input tests, bridge mapper/routing tests, and workspace build/tests pass; a dependency check confirms no `harbor-terminal` → `harbor-widget` edge.

## Out of Scope

- Changes to `CustomPaint` deferred-input behavior or a generic framework external-input handler.
- Runtime scheduler, external-paint deadlines, and cursor blink fixes.
- Moving App terminal lifecycle, PTY ownership, or cross-window policy into the bridge or `harbor-widget`.
- Extracting `terminal-core`, removing winit, or creating a separate bridge crate.

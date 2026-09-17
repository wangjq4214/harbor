# Route IME Preedit as a Transient Terminal Overlay

**Status:** Proposed
**Date:** 2026-09-17

## Context

Harbor already routes committed IME text to the PTY and exposes host-neutral IME allowance and cursor-position effects, but winit preedit updates are retained only for duplicate-key suppression. Alternatives were to intercept composition in the application host, mutate terminal screen cells, or route transient composition through the existing focused widget and terminal boundaries.

## Decision

Route preedit updates through the platform-independent widget input path to the focused Terminal Widget Bridge, and store them as transient Terminal-owned presentation state that never enters terminal screen state or PTY encoding. Render composition from the live cursor, wrapping across visible cells and clipping to the terminal viewport; the first non-empty preedit returns scrollback to the live bottom, while commit, cancellation, IME disablement, or focus loss clears it.

Use the terminal render viewport and preedit caret geometry to publish the IME cursor position through existing RuntimeEffects and the Winit Adapter, which applies `Window::set_ime_cursor_area`; do not add a raw Win32 IME path unless winit proves insufficient during Windows validation.

## Consequences

- Only committed IME text follows the existing PTY write path; preedit cannot modify terminal contents, scrollback, pending-wrap, or protocol state.
- Focus routing naturally sends composition to the active terminal and keeps generic native-window handling in the Winit Adapter.
- Render and cursor-area calculations share terminal cell metrics, allocation origin, scaling, scroll, and resize geometry.
- The implementation must add host-neutral preedit events, transient Terminal composition state, overlay rendering, IME position effects, cancellation handling, and Windows dogfood evidence.

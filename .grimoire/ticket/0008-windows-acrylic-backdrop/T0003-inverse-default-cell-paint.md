# Inverse Default Cell paint

**Ticket ID:** T0003
**Source:** [Spec: 0008-windows-acrylic-backdrop](../../spec/0008-windows-acrylic-backdrop.md)
**Status:** Todo

## Goal

Inverse Default Cells render as an opaque default-foreground fill with `BACKGROUND` RGB glyphs so reverse video stays readable over Acrylic.

## Layers

- [ ] **Config:** None — this slice consumes existing `BACKGROUND`; it does not change the constant.
- [ ] **Terminal:** Inverse+`Color::Default`/`Color::Default` cells fill with opaque default foreground in the background layer; `glyph_color` uses `BACKGROUND` RGB at alpha 1 instead of opaque white. Non-default cell backgrounds stay fully opaque. Do not change `Color::to_rgba(Color::Default)`.
- [ ] **Winit Runtime Integration:** None — clear color remains the T0001 translucent `BACKGROUND`.
- [ ] **Runtime Host:** None — no window or compositor changes.
- [ ] **Verification:** Update and add focused tests for `glyph_color` and inverse default background quads; Default Background Cell skip stays degenerate for non-inverse default cells.

## Approach

1. In `crates/harbor-terminal/src/render/text.rs`, change inverse-default `glyph_color` so `INVERSE` plus default background uses `harbor_config::BACKGROUND` RGB with alpha 1.0, not `[1, 1, 1, 1]`.
2. In `crates/harbor-terminal/src/render/background.rs`, treat inverse cells whose fg and bg are both `Color::Default` as a filled quad using default foreground (`Color::Default.to_rgba()`, still opaque white), not a degenerate skip.
3. Keep skipping non-inverse Default Background Cells so Acrylic remains visible.
4. Replace `default_colors_preserve_attributes` and add background-layer tests that match the new inverse-default contract.

## Blocked by

- T0001 — Inverse readability is specified over the Acrylic clear, not an opaque sheet.

## Blocks

- T0004 — Closing Win10 smoke and P7 docs include inverse cells in the full main-window look.

## Acceptance

- [ ] Inverse Default Cells fill with opaque default foreground (white).
- [ ] Inverse Default Cell glyphs use `BACKGROUND` RGB at alpha 1.
- [ ] Non-inverse Default Background Cells still skip fill (degenerate quads).
- [ ] Named or RGB cell backgrounds remain opaque and readable.
- [ ] `Color::to_rgba(Color::Default)` is still opaque white.

## Out of Scope

- OSC 10/11/12 default-color protocol.
- Changing global `Color::to_rgba` semantics.
- Caption chrome (T0002).
- Win10 accent API (T0004).

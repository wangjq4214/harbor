# Terminal Decoration Preset

**Ticket ID:** T0006
**Source:** [Spec: 0009-widget-decoration-and-terminal-chrome](../../spec/0009-widget-decoration-and-terminal-chrome.md), refined by conversation to preserve the implemented 2dp inset
**Status:** Todo

## Goal

The main Harbor Terminal visibly uses the confirmed 12dp anti-aliased radius and outer shadow while preserving its actual 2dp inset, allocation behavior, Acrylic transparency, input, and scheduling.

## Layers

- [ ] **Widget API and Layout:** Compose `Padding::all(2.0)` with `DecoratedBox` around `TerminalWidgetBridge`; use no decoration fill and preserve the Terminal allocation inside the existing 2dp inset.
- [ ] **Fiber and Retained Scene:** Consume the established shadow/background/child/border ordering and rounded child clip without adding Terminal-specific scene types.
- [ ] **Widget Renderer and Frame Encoder:** Consume the completed shadow and external rounded-clip paths; no new renderer contract should be introduced in this final composition slice.
- [ ] **Runtime Host and Terminal Bridge:** Update the main Runtime root composition only; keep bridge handlers, external IDs, Terminal ownership, confirmation window, and standalone host unchanged.
- [ ] **Verification:** Add root-composition assertions and perform interactive DPI, Acrylic, resize, input, scrollbar, selection, blink, synchronized-output, and surface-recovery checks.

## Approach

1. Replace the main root's direct `Padding::all(2.0).child(bridge)` composition with the same 2dp padding containing a `DecoratedBox` that wraps the bridge.
2. Configure 12dp uniform radius, one 25%-opaque black outer shadow, `(0dp, 4dp)` offset, 12dp blur, zero spread, no color fill, no border, and `ClipBehavior::AntiAlias`.
3. Keep the preset at the Runtime Host composition boundary so `harbor-terminal` and `TerminalWidgetBridge` expose no visual-style properties.
4. Assert the composed root preserves the 2dp inset and Terminal allocation rather than adopting the source spec's stale 16dp description.
5. Verify Default Background Cells still reveal Acrylic and explicit/inverse backgrounds remain readable under the clip.
6. Run product-level interaction, scheduling, DPI, resize, and recovery checks against the decorated terminal.

## Blocked by

- T0001 — Supplies the public decoration values used by the preset.
- T0003 — Supplies the outer-shadow rendering required by the preset.
- T0005 — Supplies rounded anti-aliased clipping for Terminal's external draw.

## Blocks

- (none)

## Acceptance

- [ ] The main Terminal displays a 12dp anti-aliased rounded outline and a 25%-opaque black shadow with 4dp downward offset, 12dp blur, and zero spread.
- [ ] The application root still uses `Padding::all(2.0)` and the decoration does not change the Terminal's measured allocation beyond that existing inset.
- [ ] Default Background Cells remain unfilled so the configured Windows Acrylic material is visible through the terminal.
- [ ] Explicit cell backgrounds, inverse cells, cursor, selection, scrollbar, and text remain readable and are clipped only at rounded corners.
- [ ] Typing, IME, focus, pointer selection, wheel input, scrollbar dragging, paste gating, cursor blink, and synchronized output behave as before.
- [ ] Resize, maximize/restore, fractional DPI, zero-size suspension, and surface recovery preserve the decoration without validation errors or continuous redraw.
- [ ] Paste confirmation and standalone Terminal appearances remain unchanged.
- [ ] The reference 80×24 terminal frame remains within the spec's sub-2ms p95 performance target.

## Out of Scope

- Changing the root inset from the implemented and reconfirmed 2dp to the source spec's stale 16dp value.
- User-configurable radius, shadow, border, or Acrylic settings.
- A background fill or visible border in the product preset.
- Decoration of the paste confirmation window or standalone terminal adapter.
- Terminal-owned style properties, renderer changes, custom caption chrome, or compositor-policy changes.

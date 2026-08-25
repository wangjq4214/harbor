# Main-window compositing Acrylic

**Ticket ID:** T0001
**Source:** [Spec: 0008-windows-acrylic-backdrop](../../spec/0008-windows-acrylic-backdrop.md)
**Status:** Todo

## Goal

On Windows 11 build 22621 or later, empty Default Background Cells in the Harbor main window show TransientWindow Acrylic instead of an opaque brown sheet.

## Layers

- [ ] **Config:** Set `harbor_config::BACKGROUND` alpha to 0.72; keep RGB `[0.36, 0.20, 0.08]`.
- [ ] **Terminal:** Select a compositing-capable `CompositeAlphaMode` for the main `GpuContext` surface (prefer `PreMultiplied`, then `PostMultiplied`, then `Auto`; never `Opaque` when a compositing mode is advertised).
- [ ] **Winit Runtime Integration:** None — presenter already clears with the injected `WinitFrameTarget` color; Host supplies a premultiplied clear when the surface is premultiplied.
- [ ] **Runtime Host:** Create the main window with `with_transparent(true)` and `with_system_backdrop(BackdropType::TransientWindow)` when build ≥ 22621; skip opaque `paint_gdi_background` on that path; pass translucent `BACKGROUND` as the frame clear color.
- [ ] **Verification:** Unit tests for alpha-mode selection and `BACKGROUND` alpha; existing GPU/surface recovery tests stay green; do not change confirmation-window surface config.

## Approach

1. Change `BACKGROUND` in `crates/harbor-config` to alpha 0.72 and update any tests that assert opaque alpha.
2. Add a focused `select_compositing_alpha_mode` helper in `crates/harbor-terminal/src/render/gpu.rs` and use it from `GpuContext::new` only (confirmation keeps `alpha_modes[0]` in `src/app/confirmation.rs`).
3. On Windows, detect OS build (for example `RtlGetVersion`); at ≥ 22621 set winit `with_transparent(true)` and `WindowAttributesExtWindows::with_system_backdrop(BackdropType::TransientWindow)` on the main window only.
4. Skip `paint_gdi_background` when the main window is translucent so the opaque GDI fill cannot cover Acrylic.
5. When converting `BACKGROUND` to `wgpu::Color` for the main `WinitFrameTarget`, premultiply RGB by alpha if the configured surface mode is `PreMultiplied`.
6. Leave paste confirmation, standalone terminal adapter, and Mica unused.

## Blocked by

- None — first observable slice; no pre-refactoring ticket.

## Blocks

- T0002 — Caption chrome lands on the translucent main HWND created here.
- T0003 — Inverse Default Cell paint is verified over this Acrylic clear.
- T0004 — Win10 accent reuses transparency, compositing alpha, and skipped GDI.

## Acceptance

- [ ] `BACKGROUND` is `[0.36, 0.20, 0.08, 0.72]`.
- [ ] Main-surface alpha-mode tests prove a compositing mode is chosen whenever one is advertised, and `Opaque` is not chosen in that case.
- [ ] On Windows 11 22621+, a manual smoke shows the desktop or another window through empty default-background cells as Acrylic, not Mica and not an opaque sheet.
- [ ] Confirmation-window `alpha_mode` selection and black clear are unchanged.
- [ ] Existing surface recovery and idle-`Wait` gates still pass.

## Out of Scope

- Hiding caption text or icon (T0002).
- Inverse Default Cell fill/glyph changes (T0003).
- `SetWindowCompositionAttribute` Win10 path (T0004).
- Acrylic on confirmation or standalone host.
- TOML, settings UI, or runtime Acrylic toggle.

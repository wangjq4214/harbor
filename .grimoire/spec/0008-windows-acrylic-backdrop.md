# Windows Acrylic Backdrop

**Spec ID:** 0008
**Status:** Implemented
**Date:** 2026-08-25

**Superseded in part:** the earlier TransientWindow-first, 0.72-alpha contract in the acceptance text below is superseded by the unified backdrop tint chain (ADR 0026, amended by ADR 0028). The current four-tier chain and the configured `BACKGROUND` alpha are the reference; the earlier values are retained as historical design intent.
## Requirement

The Harbor main window on Windows must show a Windows Terminal-style Acrylic backdrop through default-background cells and the caption strip, while remaining readable for inverse and non-default cells.

## Solution

Acrylic is a Runtime Host / compositor concern, not a VT protocol feature. The Host creates the main window as an alpha-composited HWND, enables the OS acrylic material, and presents GPU frames whose default-background pixels do not occlude that material. Terminal continues to skip Default Background Cell quads and paints Inverse Default Cells so reverse video stays readable.

On Windows 11 build 22621 and later, the Host creates the main window with winit transparency, `BackdropType::TransientWindow` (`DWMSBT_TRANSIENTWINDOW`), and caption color `None` so Desktop Acrylic sits behind the entire window including the non-client strip. On earlier Windows versions, including Windows 10, the Host applies `SetWindowCompositionAttribute` with `ACCENT_ENABLE_ACRYLICBLURBEHIND` after the HWND exists, using a tint aligned to `harbor_config::BACKGROUND` at alpha 0.72. Mica (`DWMSBT_MAINWINDOW`) is not used.

The Host keeps system decorations. It leaves the window title `Harbor` for the taskbar and Alt-Tab, and uses `SetWindowThemeAttribute` with `WTNCA_NODRAWCAPTION` and `WTNCA_NODRAWICON` so DWM does not paint title text or the caption icon. Minimize, maximize, and close remain DWM System Caption Chrome; Harbor does not custom-draw or hit-test those buttons. Windows 10 Caption Degradation is accepted: if a theme still paints an opaque caption, the client area must still be acrylic and the limitation is documented.

The main-window wgpu surface selects a compositing-capable `CompositeAlphaMode`, preferring `PreMultiplied`, then `PostMultiplied`, then `Auto`, and never choosing `Opaque` when a compositing mode is advertised. The Host passes `BACKGROUND` with alpha 0.72 as `WinitFrameTarget` clear color (premultiplied at the clear boundary when the surface is premultiplied). Opaque GDI first-paint is skipped whenever the main window is translucent. Widget chrome must not emit an opaque full-window quad over the terminal.

`Color::to_rgba(Color::Default)` remains opaque white. Inverse Default Cells fill with that default foreground and glyph-paint `BACKGROUND` RGB at alpha 1. Non-default cell backgrounds stay fully opaque.

The paste confirmation window stays a separate opaque OS window with its existing surface alpha-mode selection and black clear. The standalone terminal winit/wgpu adapter stays opaque.

## Decisions

Current compositor policy is [ADR 0026](../adr/0026-unified-window-backdrop-tint-chain.md), amended by [ADR 0028](../adr/0028-dual-composition-target-stack.md); the original TransientWindow-first/0.72 preset above is historical. Keep [system caption buttons](../adr/0024-system-caption-buttons-undrawn-title.md). Windows 10 opaque-caption degradation is accepted/documented, not grounds for custom caption buttons.

## Test Plan

Verify Windows 11 empty-cell/caption Acrylic, Windows 10 client Acrylic with documented opaque-caption degradation, readable inverse/colored cells without black holes, opaque separate confirmation, and restored transparency after surface recovery.

- **Integration tests:** Alpha-mode selection prefers a compositing-capable mode and never picks `Opaque` when alternatives exist; Inverse Default Cell fill and glyph color; Default Background Cell skip remains degenerate; `BACKGROUND` alpha is 0.72; existing GPU/surface recovery contracts stay green.
- **Manual tests:** Windows 11 22H2+ smoke that Acrylic is visible through empty cells and the caption strip; Windows 10 smoke that the client is Acrylic and that an opaque caption, if present, is the documented degradation; taskbar still says Harbor; min/max/close work; paste confirmation still presents; system Transparency effects off yields a defined non-crash appearance.
- **Performance thresholds:** Idle scheduling remains `Wait` with no continuous redraw loop introduced by Acrylic. Present and input-latency gates in `docs/validation.md` stay green.
- **Edge cases:** Maximize and restore; DPI change; zero-sized then drawable; surface lost/outdated; inverse on named vs default colors; Windows builds below 22621 taking the accent path; confirmation window open while the main window is Acrylic.

## Out of Scope

- User TOML, theme packs, settings UI, or a runtime Acrylic toggle.
- Custom blur radius, tint controls, or non-system blur materials.
- Acrylic, transparency, or alpha-mode changes on the paste confirmation dialog.
- Unix transparency.
- OSC 10/11/12 default-color protocol.
- Mica or TabbedWindow backdrops.
- Custom-drawn caption buttons, client-side decoration, or empty `with_title("")` that would blank the taskbar name.
- Changing `Color::to_rgba(Color::Default)` globally.
- Acrylic on the standalone terminal winit/wgpu adapter.
- Changing confirmation-window `alpha_modes[0]` selection unless a black/opaque regression is proven after the main-surface change.

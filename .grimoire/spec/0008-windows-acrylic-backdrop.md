# Windows Acrylic Backdrop

**Spec ID:** 0008
**Status:** In Progress
**Date:** 2026-08-25

## Requirement

The Harbor main window on Windows must show a Windows Terminal-style Acrylic backdrop through default-background cells and the caption strip, while remaining readable for inverse and non-default cells.

## Solution

Acrylic is a Runtime Host / compositor concern, not a VT protocol feature. The Host creates the main window as an alpha-composited HWND, enables the OS acrylic material, and presents GPU frames whose default-background pixels do not occlude that material. Terminal continues to skip Default Background Cell quads and paints Inverse Default Cells so reverse video stays readable.

On Windows 11 build 22621 and later, the Host creates the main window with winit transparency, `BackdropType::TransientWindow` (`DWMSBT_TRANSIENTWINDOW`), and caption color `None` so Desktop Acrylic sits behind the entire window including the non-client strip. On earlier Windows versions, including Windows 10, the Host applies `SetWindowCompositionAttribute` with `ACCENT_ENABLE_ACRYLICBLURBEHIND` after the HWND exists, using a tint aligned to `harbor_config::BACKGROUND` at alpha 0.72. Mica (`DWMSBT_MAINWINDOW`) is not used.

The Host keeps system decorations. It leaves the window title `Harbor` for the taskbar and Alt-Tab, and uses `SetWindowThemeAttribute` with `WTNCA_NODRAWCAPTION` and `WTNCA_NODRAWICON` so DWM does not paint title text or the caption icon. Minimize, maximize, and close remain DWM System Caption Chrome; Harbor does not custom-draw or hit-test those buttons. Windows 10 Caption Degradation is accepted: if a theme still paints an opaque caption, the client area must still be acrylic and the limitation is documented.

The main-window wgpu surface selects a compositing-capable `CompositeAlphaMode`, preferring `PreMultiplied`, then `PostMultiplied`, then `Auto`, and never choosing `Opaque` when a compositing mode is advertised. The Host passes `BACKGROUND` with alpha 0.72 as `WinitFrameTarget` clear color (premultiplied at the clear boundary when the surface is premultiplied). Opaque GDI first-paint is skipped whenever the main window is translucent. Widget chrome must not emit an opaque full-window quad over the terminal.

`Color::to_rgba(Color::Default)` remains opaque white. Inverse Default Cells fill with that default foreground and glyph-paint `BACKGROUND` RGB at alpha 1. Non-default cell backgrounds stay fully opaque.

The paste confirmation window stays a separate opaque OS window with its existing surface alpha-mode selection and black clear. The standalone terminal winit/wgpu adapter stays opaque.

### Seams

| Seam | Connects | Expects | Provides |
| --- | --- | --- | --- |
| Main-window compositor policy | Runtime Host → Windows DWM / user32 | HWND, OS build, system decorations | Acrylic behind client and caption; undrawn title and icon; DWM caption buttons |
| Surface compositing alpha | Runtime Host / `GpuContext` → wgpu | Advertised `alpha_modes` | Compositing-capable `CompositeAlphaMode` for the main surface only |
| Frame clear color | Runtime Host → Runtime Frame Presentation | `BACKGROUND` at alpha 0.72, premultiplied when required | `LoadOp::Clear` that reveals acrylic through Default Background Cells |
| Default and inverse cell paint | `harbor-terminal` → `harbor_config::BACKGROUND` | `Color::Default` cells and inverse-default attributes | Skipped default-background quads; readable Inverse Default Cells |

## End-to-End Tests

### E2E: Windows 11 acrylic through empty cells

- **Given:** Harbor is running on Windows 11 build 22621 or later, with system transparency effects enabled, and the screen contains Default Background Cells.
- **When:** The user places another window or the desktop behind Harbor.
- **Then:** The desktop or window behind shows through empty cells as Acrylic, not as an opaque brown sheet and not as Mica.

### E2E: Caption strip is acrylic without title or icon

- **Given:** The main window is visible with system decorations.
- **When:** The user inspects the caption strip and uses minimize, maximize, and close.
- **Then:** Caption text and icon are not drawn, the strip is Acrylic (Windows 11), the three system buttons work, and the taskbar / Alt-Tab still identify the window as Harbor.

### E2E: Inverse and colored cells stay readable

- **Given:** The screen contains Inverse Default Cells and cells with named or RGB backgrounds.
- **When:** A frame is presented over Acrylic.
- **Then:** Inverse Default Cells are opaque default-foreground fills with `BACKGROUND`-colored glyphs, and non-default backgrounds remain opaque and readable with no black holes.

### E2E: Windows 10 client acrylic with accepted caption degradation

- **Given:** Harbor is running on Windows 10 with transparency effects enabled.
- **When:** The user places content behind the window.
- **Then:** Default Background Cells show Acrylic through the client area; if the caption strip stays opaque, Harbor does not crash, does not custom-draw buttons, and the limitation matches documented Windows 10 Caption Degradation.

### E2E: Paste confirmation remains an opaque separate window

- **Given:** A confirmable paste has opened the native confirmation window.
- **When:** Both windows present frames.
- **Then:** The confirmation window stays opaque with its existing present path; Acrylic does not apply to it; confirmed and cancelled paste behavior is unchanged.

### E2E: Surface recovery still presents

- **Given:** The main window is presenting with a compositing-capable alpha mode.
- **When:** The surface is lost, outdated, resized, or temporarily zero-sized, then becomes drawable again.
- **Then:** Runtime Frame Presentation recovers or skips per existing policy, and subsequent frames again reveal Acrylic through Default Background Cells.

## Decisions

### Acrylic on both Windows 10 and Windows 11, not Mica

- **Choice:** Win11 uses `DWMSBT_TRANSIENTWINDOW`; Win10 uses accent-policy Acrylic. Mica is rejected.
- **Reason:** The product look is Windows Terminal-style glass that blurs windows behind Harbor, not a Mica app frame.
- **ADR reference:** [0023-windows-acrylic-backdrop-not-mica](../adr/0023-windows-acrylic-backdrop-not-mica.md)

### System caption buttons with undrawn title and icon

- **Choice:** Keep DWM min/max/close; suppress caption text and icon; preserve the taskbar name Harbor.
- **Reason:** The caption strip must be Acrylic without a custom-drawn title bar.
- **ADR reference:** [0024-system-caption-buttons-undrawn-title](../adr/0024-system-caption-buttons-undrawn-title.md)

### Windows 10 opaque caption is documented degradation, not CSD

- **Choice:** If some Win10 themes leave an opaque caption, document it and do not custom-draw buttons.
- **Reason:** ADR 0023 left an "unless caption cannot reveal acrylic" opening for CSD; ADR 0024 forbids custom caption-button painting. This spec closes that opening toward documentation, matching the confirmed Windows 10 Caption Degradation contract.
- **ADR reference:** [0023-windows-acrylic-backdrop-not-mica](../adr/0023-windows-acrylic-backdrop-not-mica.md), [0024-system-caption-buttons-undrawn-title](../adr/0024-system-caption-buttons-undrawn-title.md)

### Host owns compositor and surface config; Runtime owns present given a clear color

- **Choice:** Window attributes, DWM/accent policy, GDI first-paint, and main-surface `alpha_mode` stay in the Runtime Host / `GpuContext`. Translucent clear color is injected through `WinitFrameTarget`. Terminal only changes default-background skip and Inverse Default Cell paint.
- **Reason:** App remains long-term owner of Window and Surface; the winit integration already presents with a host-supplied clear color; Terminal must not own the HWND or surface.
- **ADR reference:** [0015-runtime-owned-frame-presentation](../adr/0015-runtime-owned-frame-presentation.md), [0011-terminal-custompaint-gpu-injection](../adr/0011-terminal-custompaint-gpu-injection.md), [0012-consolidate-render-ui-into-terminal](../adr/0012-consolidate-render-ui-into-terminal.md)

### Paste confirmation and standalone host stay opaque

- **Choice:** Do not change confirmation-window alpha-mode selection or apply Acrylic to the standalone terminal adapter.
- **Reason:** Confirmation is a separate OS window; the standalone adapter is a test/host seam, not the product window.
- **ADR reference:** [0007-retain-separate-paste-confirmation-window](../adr/0007-retain-separate-paste-confirmation-window.md), [0009-app-cross-window-input-gate](../adr/0009-app-cross-window-input-gate.md), [0021-external-draw-scheduling-and-standalone-terminal-host](../adr/0021-external-draw-scheduling-and-standalone-terminal-host.md)

### Hardcoded translucent `BACKGROUND` until user config exists

- **Choice:** Keep RGB `[0.36, 0.20, 0.08]` and set alpha to 0.72 in `harbor_config`. No TOML or runtime toggle in this spec.
- **Reason:** User-configurable background alpha is a separate config issue; Acrylic must still have a translucent default to be visible.
- **ADR reference:** [0023-windows-acrylic-backdrop-not-mica](../adr/0023-windows-acrylic-backdrop-not-mica.md)

## Test Plan

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

## Future Evolution

- Land user-configurable background alpha and optional Acrylic disable in the TOML config issue.
- Revisit Windows 10 caption reliability if dogfood shows the opaque strip is common enough to need a non-CSD Host workaround.
- Offer Mica as an explicit appearance option only if product direction changes away from Windows Terminal-style glass.
- Extend Acrylic to Unix only after P8 host work exists; this spec is Windows-only.
- Re-evaluate undocumented accent policy if Microsoft removes `SetWindowCompositionAttribute` behavior.

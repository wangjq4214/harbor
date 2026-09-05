# Dual Composition Target Stack for Windows Acrylic

**Status:** Completed
**Date:** 2026-09-05

## Context

The WASDK backdrop integration attempted to share one topmost composition target by casting a `Windows.UI.Composition.ContainerVisual` to the incompatible classic DirectComposition `IDCompositionVisual` required by wgpu. The first correction split WASDK and wgpu into the two composition-target layers that Windows permits on one HWND. Runtime evidence then showed that this did not restore Acrylic on the affected machine because the Windows App SDK bootstrap failed, so the dual-target path never ran and the app selected legacy AccentPolicy instead.

## Decision

Bind `DesktopAcrylicController` to a lower `topmost=false` `DesktopWindowTarget` and render wgpu's transparent swap chain through a separate upper `topmost=true` classic DirectComposition target on the same HWND. When the Windows App Runtime is unavailable on Windows 11 22H2 or newer, restore `DWMSBT_TRANSIENTWINDOW` as the native Acrylic fallback; reserve AccentPolicy for older Windows builds. The backdrop backend continues to own native resources for the window lifetime and does not export a shared visual to the renderer.

## Consequences

- The WASDK path uses distinct lower backdrop and upper renderer composition slots.
- The implementation removes the unsupported `ContainerVisual` to `IDCompositionVisual` cast and the `composition_visual` handoff.
- Machines without the Windows App Runtime use verified DWM TransientWindow Acrylic instead of reporting an ineffective AccentPolicy call as success.
- Older Windows builds continue to use AccentPolicy Acrylic, then the opaque `#1E1E1E` fallback.
- `WindowBackdropStyle` remains authoritative for WASDK and AccentPolicy tint; DWM TransientWindow has no configurable compositor tint.

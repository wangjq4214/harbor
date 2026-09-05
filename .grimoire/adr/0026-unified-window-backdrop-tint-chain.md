# Unified Window Backdrop Tint Chain

**Status:** Completed
**Date:** 2026-09-01

## Context

The native caption strip and the client area showed a visible color seam: the client root painted a faint 6% white veil over the acrylic backdrop while the DWM caption strip stayed bare acrylic. `DWMSBT_TRANSIENTWINDOW` exposes no tint parameters, so the seam cannot be fixed by keeping the veil client-side. Alternatives were `DesktopAcrylicController` from the Windows App SDK (full-window tintable acrylic, including the caption), AccentPolicy with an alpha gradient (whole-window but approximate), or opaque caption colors (loses acrylic).

## Decision

Apply one compositor-level backdrop tint to the whole main window through the fallback chain amended by ADR 0028:

1. Windows App SDK initialization succeeds AND `DesktopAcrylicController::IsSupported()` → tinted acrylic via `DesktopAcrylicController`, covering the caption strip.
2. On Windows 11 22H2 or newer without a usable App Runtime → native `DWMSBT_TRANSIENTWINDOW` acrylic.
3. On older Windows builds → AccentPolicy acrylic with the same tint values.
4. Otherwise → opaque `#1E1E1E` fallback fill.

The tint is a single `WindowBackdropStyle` (white, TintOpacity 0.06, LuminosityOpacity 1.0) shared by the WASDK and AccentPolicy tiers; the terminal panel keeps its warm-brown layer, and the client root stops painting the white veil. DWM TransientWindow is retained as an untintable availability fallback. Windows App SDK bindings are self-generated with `windows-bindgen` from a pinned WinAppSDK version and vendored into the repository.

## Consequences

- ADR 0028 restores ADR 0023's Windows 11 `DWMSBT_TRANSIENTWINDOW` path only when the tintable WASDK path is unavailable.
- Windows 10 keeps AccentPolicy; Windows 11 without the App Runtime retains native Acrylic.
- Builds need no network once bindings are vendored.
- Requires a bootstrap-failure and `IsSupported()` check before window styling.

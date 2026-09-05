# Unified Window Backdrop Tint Chain

**Status:** Completed
**Date:** 2026-09-01

## Context

The native caption strip and the client area showed a visible color seam: the client root painted a faint 6% white veil over the acrylic backdrop while the DWM caption strip stayed bare acrylic. `DWMSBT_TRANSIENTWINDOW` exposes no tint parameters, so the seam cannot be fixed by keeping the veil client-side. Alternatives were `DesktopAcrylicController` from the Windows App SDK (full-window tintable acrylic, including the caption), AccentPolicy with an alpha gradient (whole-window but approximate), or opaque caption colors (loses acrylic).

## Decision

Apply one compositor-level backdrop tint to the whole main window through a three-tier chain:

1. Windows App SDK initialization succeeds AND `DesktopAcrylicController::IsSupported()` → tinted acrylic via `DesktopAcrylicController`, covering the caption strip.
2. Otherwise → AccentPolicy acrylic with the same tint values.
3. Otherwise → opaque `#1E1E1E` fallback fill.

The tint is a single `WindowBackdropStyle` (white, TintOpacity 0.06, LuminosityOpacity 1.0) shared by every tier; the terminal panel keeps its warm-brown layer, and the client root stops painting the white veil. Windows App SDK bindings are self-generated with `windows-bindgen` from a pinned WinAppSDK version and vendored into the repository. Distribution stays framework-dependent: a missing Windows App Runtime simply falls to tier 2.

## Consequences

- Supersedes ADR 0023's Windows 11 `DWMSBT_TRANSIENTWINDOW` path and ADR 0025's host-root white veil.
- Windows 10 keeps AccentPolicy; Windows 11 without the App Runtime degrades to the same tier.
- Builds need no network once bindings are vendored.
- Requires a bootstrap-failure and `IsSupported()` check before window styling.

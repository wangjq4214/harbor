# Windows Acrylic Backdrop, Not Mica

**Status:** Completed
**Date:** 2026-08-25

## Context

Issue 121 needs a translucent Harbor main window whose default-background cells reveal the desktop. Alternatives were Windows 11 Mica for main-window chrome, Windows 11 TransientWindow acrylic, an opaque Windows 10 fallback, or Windows 10 accent-policy acrylic.

## Decision

Use Acrylic on both Windows 10 and Windows 11 so windows and desktop behind Harbor show through, matching Windows Terminal's glass look rather than a Mica app frame. The system title bar is included; the paste confirmation window stays opaque.

## Consequences

- Windows 11 uses DWM `DWMSBT_TRANSIENTWINDOW`, not `DWMSBT_MAINWINDOW` (Mica).
- Windows 10 uses the platform acrylic composition API instead of remaining opaque.
- wgpu must present with a compositing-capable alpha mode and a translucent clear color.
- A custom client-side title bar is out of scope unless the system caption cannot reveal acrylic.

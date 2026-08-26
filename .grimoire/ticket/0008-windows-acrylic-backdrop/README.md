# Windows Acrylic Backdrop

**Source:** [Spec: 0008-windows-acrylic-backdrop.md](../../spec/0008-windows-acrylic-backdrop.md)
**Ticket folder:** `.grimoire/ticket/0008-windows-acrylic-backdrop/`

## Overview

These tickets give the Harbor main window a Windows Terminal-style Acrylic backdrop: default-background cells and the caption strip reveal content behind the window, inverse and colored cells stay readable, and the paste confirmation window stays opaque. Win11 22621+ uses DWM TransientWindow; earlier Windows uses accent-policy Acrylic. System min/max/close stay DWM-drawn; caption text and icon are not painted.

## Layers

The project's architectural layers confirmed during decomposition:

1. **Config** — `harbor-config` appearance constants such as `BACKGROUND`.
2. **Terminal** — `harbor-terminal` GPU surface configuration and cell paint.
3. **Winit Runtime Integration** — borrowed-frame clear and present via `WinitFrameTarget`.
4. **Runtime Host** — `src/` window lifetime, DWM/accent compositor policy, and GDI first-paint.
5. **Verification** — crate tests, docs, and Windows smoke.

Every ticket includes all five layers and states why a layer has no work when that is the case.

## Dependency Graph

```text
T0001
  ├─→ T0002 ─┐
  └─→ T0003 ─┴─→ T0004
```

### Blocking relationships

| Ticket | Blocks | Reason |
| --- | --- | --- |
| T0001 | T0002, T0003, T0004 | Caption, inverse paint, and Win10 accent all require the compositing stack (translucent `BACKGROUND`, compositing alpha, Host transparency, skipped GDI). |
| T0002 | T0004 | Both edit main-window creation in `src/app.rs`; Win10 accent must land on the finished caption-chrome path. |
| T0003 | T0004 | Closing Win10 smoke and P7 docs verify the full main-window look, including Inverse Default Cells over Acrylic. |
| T0004 | — | Final Host fallback and documentation slice. |

### Parallel groups

| Group | Tickets | Reason |
| --- | --- | --- |
| A | T0002, T0003 | After T0001: T0002 edits Host window attributes; T0003 edits `harbor-terminal` text/background paint. No shared files or runtime contracts. |

## Recommended Order

1. T0001 — Main-window compositing Acrylic (Win11)
2. T0002 and T0003 — Caption chrome and Inverse Default Cells (parallel)
3. T0004 — Windows 10 accent Acrylic and documentation

## Ticket Index

| Ticket ID | File | Title | Summary |
| --- | --- | --- | --- |
| T0001 | [T0001-main-window-compositing-acrylic.md](./T0001-main-window-compositing-acrylic.md) | Main-window compositing Acrylic | Win11 TransientWindow glass through default-background cells. |
| T0002 | [T0002-system-caption-chrome.md](./T0002-system-caption-chrome.md) | System caption chrome | Undrawn title and icon; DWM min/max/close; acrylic caption strip. |
| T0003 | [T0003-inverse-default-cell-paint.md](./T0003-inverse-default-cell-paint.md) | Inverse Default Cell paint | Opaque default-foreground fill and `BACKGROUND` glyphs for inverse+default cells. |
| T0004 | [T0004-windows-10-accent-and-docs.md](./T0004-windows-10-accent-and-docs.md) | Windows 10 accent and docs | Accent-policy Acrylic below 22621; documented caption degradation; P7 pointer. |

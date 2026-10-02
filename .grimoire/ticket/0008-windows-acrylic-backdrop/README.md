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
T0001 ─→ T0002 ─→ T0004
   └────────────→ T0004
```

### Blocking relationships

| Ticket | Blocks | Reason |
| --- | --- | --- |
| T0001 | T0002, T0004 | Caption and Win10 accent require the compositing stack (translucent `BACKGROUND`, compositing alpha, Host transparency, skipped GDI). |
| T0002 | T0004 | Both edit main-window creation in `src/app.rs`; Win10 accent must land on the finished caption-chrome path. |
| T0004 | — | Final Host fallback and documentation slice. |

### Contract prerequisite

T0004 smoke/docs must also verify the [spec's inverse-cell contract](../../spec/0008-windows-acrylic-backdrop.md#e2e-inverse-and-colored-cells-stay-readable) over the Acrylic clear; this is a behavior prerequisite, not a separate ticket dependency.

## Recommended Order

1. T0001 — Main-window compositing Acrylic (Win11)
2. T0002 — Caption chrome
3. T0004 — Windows 10 accent Acrylic and documentation

## Ticket Index

| Ticket ID | File | Title | Summary |
| --- | --- | --- | --- |
| T0001 | [T0001-main-window-compositing-acrylic.md](./T0001-main-window-compositing-acrylic.md) | Main-window compositing Acrylic | Win11 TransientWindow glass through default-background cells. |
| T0002 | [T0002-system-caption-chrome.md](./T0002-system-caption-chrome.md) | System caption chrome | Undrawn title and icon; DWM min/max/close; acrylic caption strip. |
| T0004 | [T0004-windows-10-accent-and-docs.md](./T0004-windows-10-accent-and-docs.md) | Windows 10 accent and docs | Accent-policy Acrylic below 22621; documented caption degradation; P7 pointer. |

## Historical Contract Destinations

T0003 (Inverse Default Cell paint) retains its stable historical ID here; its contract is preserved in [spec 0008 Solution](../../spec/0008-windows-acrylic-backdrop.md#solution), [inverse-cell E2E](../../spec/0008-windows-acrylic-backdrop.md#e2e-inverse-and-colored-cells-stay-readable), and [Test Plan](../../spec/0008-windows-acrylic-backdrop.md#test-plan). This historical mapping does not mark integrated acceptance passed.

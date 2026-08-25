# System Caption Buttons With Undrawn Title

**Status:** Proposed
**Date:** 2026-08-25

## Context

The acrylic main window needs a caption strip that matches the glass client area. Alternatives were a normal titled caption, an empty window title, a fully custom-drawn title bar, or keeping DWM caption buttons while suppressing caption text.

## Decision

Keep the system minimize, maximize, and close buttons and do not custom-draw them. Do not paint the window title or icon on the caption; the caption strip is acrylic. The taskbar and Alt-Tab name remain "Harbor".

## Consequences

- Host uses theme/DWM attributes such as `WTNCA_NODRAWCAPTION` rather than `with_title("")`, so the taskbar label is preserved.
- Custom caption-button hit testing and painting stay out of scope.
- Windows 10 may still paint an opaque caption on some themes; that residual is a separate product call.

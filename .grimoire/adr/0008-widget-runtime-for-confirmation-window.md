# Use Widget Runtime for the Paste Confirmation Window

**Status:** Proposed
**Date:** 2026-07-24

## Context

The retained separate paste confirmation window needs a UI implementation. The alternatives are retaining its legacy `harbor-ui` implementation or rendering the window with a separate `harbor-widget::Runtime` instance.

## Decision

Render the paste confirmation window with its own `harbor-widget::Runtime`. The App owns that window's winit, Surface, redraw, and presentation lifecycle, while its Widget Runtime uses the shared Device, Queue, and `harbor-text` CPU core.

## Consequences

- The Phase 3 text, button, focus, and preview features are exercised by the confirmation UI despite its separate surface.
- Main-window and confirmation-window frames are encoded and presented separately.
- Widget Runtime instances must not own windows, surfaces, or GPU submission.

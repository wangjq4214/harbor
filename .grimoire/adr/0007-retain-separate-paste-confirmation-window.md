# Retain a Separate Paste Confirmation Window

**Status:** Proposed
**Date:** 2026-07-24

## Context

The Phase 3 design proposed replacing the paste confirmation window with a main-window Widget Runtime overlay. The alternatives are an in-app modal, an operating-system-native dialog, or an independent application window.

## Decision

Paste confirmation uses a separate OS-level winit window rather than a main-window overlay, so it behaves as an independent native application window. This supersedes the Phase 3 removal of the separate confirmation window.

## Consequences

- Phase 3 acceptance criteria and plans must no longer require a same-frame main-window paste overlay.
- The App must manage the confirmation window's surface, redraw, and lifecycle separately from the main window.
- The confirmation window's rendering integration with the Widget Runtime remains a separate design decision.

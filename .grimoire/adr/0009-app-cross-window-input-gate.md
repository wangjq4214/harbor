# App Cross-Window Input Gate

**Status:** Proposed
**Date:** 2026-07-24

## Context

ADR 0006 relied on a modal `FocusScope` in the same Widget Runtime to keep terminal input from reaching `CustomPaint`. ADRs 0007 and 0008 instead place paste confirmation in a separate native window and Runtime, so a Runtime-local focus scope cannot prevent main-window terminal dispatch.

## Decision

While a paste confirmation window exists, the App gates terminal keyboard input and new paste requests before dispatching them to the main Runtime or PTY. The confirmation Runtime's `FocusScope` manages only its own controls; when the App gate permits terminal input, the existing provider-by-ID path still receives it after Runtime event propagation.

## Consequences

- Cross-window paste safety is enforced at the only owner of both window event streams and the PTY.
- The confirmation Runtime remains independent of the terminal Runtime.
- Terminal output, rendering, scrollback browsing, and copying remain available while the gate is active.

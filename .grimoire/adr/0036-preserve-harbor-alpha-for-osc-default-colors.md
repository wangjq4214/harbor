# Preserve Harbor Alpha for OSC Default Colors

**Status:** Proposed
**Date:** 2026-09-17

## Context

OSC 10/11/12 provide RGB default foreground, background, and cursor colors, while Harbor's startup palette also carries alpha for acrylic background and cursor appearance. Reset could target either compiled defaults or the terminal's startup-configured palette, and broad xterm parsing would add named and floating-point color forms beyond Harbor's bounded color model.

## Decision

Accept only `#RRGGBB` and `rgb:R/G/B` with one to four hexadecimal digits per component. Sets replace RGB while preserving the active slot's alpha; queries return `rgb:rrrr/gggg/bbbb` with the request's BEL or ST terminator; OSC 110/111/112 restore the complete startup-configured RGBA slot. Reject invalid, named, `rgbi:`, alpha-bearing, and multi-color payloads without changing active colors.

## Consequences

- OSC color changes preserve Harbor's acrylic and translucent-cursor policies.
- Resets honor per-terminal startup configuration rather than hard-coded palette defaults.
- Query replies remain bounded, deterministic, and compatible with common xterm RGB reply practice.
- Supporting additional xterm color forms or sequential multi-color payloads requires a later explicit contract extension.

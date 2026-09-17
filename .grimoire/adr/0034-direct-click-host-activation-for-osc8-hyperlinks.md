# Activate OSC 8 Hyperlinks Through the Host on Direct Click

**Status:** Proposed
**Date:** 2026-09-17

## Context

Issue #98 requires OSC 8 link targets to remain inert unless the host provides an explicit activation policy. Harbor also needs a concrete interaction contract: requiring a modifier reduces accidental activation, while ordinary primary-button activation matches conventional clickable UI.

## Decision

A primary-button click on an OSC 8 hyperlink directly requests activation without requiring Ctrl. The terminal owns hyperlink hit testing and returns the validated target, while the application host owns invoking the operating system's external URI handler; parsing and rendering do not fetch targets or open them without a user click. Direct activation is allowed only for `http`, `https`, `mailto`, and `file` URIs; other schemes may remain represented as hyperlink state but are not passed to the operating system.

## Consequences

- Hyperlinks are directly clickable with the primary pointer button and do not require a keyboard modifier.
- Selection and pointer-capture behavior must distinguish a completed hyperlink click from a drag or terminal mouse-reporting interaction.
- The terminal crate remains platform neutral and exposes an activation request or target rather than depending on an operating-system launcher.
- URI validation is enforced before the host invokes the external handler; only `http`, `https`, `mailto`, and `file` schemes are activatable.

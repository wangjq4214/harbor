# Remove External Frame Appearance Clear Provider

**Status:** Completed
**Date:** 2025-08-28

## Context

The terminal tint was painted by clearing the whole surface with a color reported through the ExternalFrameAppearance provider, which bled into the window inset and rounded-corner cutouts. The terminal can instead draw its own background, and the host can own base fills at the root.

## Decision

Remove the ExternalFrameAppearance provider chain (bridge, CustomPaint, Runtime, presenter) and clear the frame transparently for runtimes that own external draws, keeping the opaque black clear for plain widget runtimes such as the confirmation dialog. The host root paints the faint white veil and the no-acrylic opaque warm-brown base.

## Consequences

- The window inset and rounded corners show the host-owned white base fill instead of the terminal tint.
- The terminal background layer draws its own backdrop-aware default-cell tint, independent of the surface clear.
- Plain widget runtimes keep their previous opaque black clear.
- Tests and contracts around frame appearance providers are removed with the mechanism.

# Local Backdrop Edge Refraction

**Status:** Testing
**Date:** 2026-10-16
**Sources:** The user requested removing search-panel shadows, thickening nonuniform highlights and adding glass distortion, then selected the recommended edge/corner-only refraction to give the glass depth while leaving the center and foreground stable.
**Supersedes:** The refraction exclusion only in [ADR 0067](./0067-widget-gradient-paints-with-application-material-presets.md). Its painting/ownership decisions remain in force.
**Related:** [ADR 0066](./0066-widget-local-backdrop-blur-with-application-glass-presets.md), [spec 0029](../spec/0029-search-panel-glass-appearance.md).

## Decision

Extend widget-owned local backdrop filtering with optional bounded edge refraction. It displaces sampled background near the shape's edges and rounded corners, fading to no displacement toward the center. Foreground text, caret, icons, geometry and hit testing are not distorted. This is a reusable rendering mechanism, not a search-aware shader or comprehensive Glass component. Existing blur-only consumers retain their behavior.

The application still owns the material composition and parameters. Search uses directionally varying edge highlights (subsequently refined to a finer edge and faint black surface under spec 0029 S9) and no outer shadow; actual local background distortion, not painted imitation, provides the new depth cue. Whole-panel warping, dynamic lighting, noise and arbitrary filters remain outside the selected change.

## Consequences

The renderer must preserve scene order, premultiplied alpha, clipping, live external drawing, bounded sampling support and DPI behavior. Runtime visual evidence is necessary in addition to GPU tests. Exact strength, edge-band width and highlight values are reversible tuning, not user-specified thresholds or physical-optics guarantees.

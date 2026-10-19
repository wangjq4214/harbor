# Shared Application Glass Presets with Local Overrides

**Status:** Testing
**Date:** 2026-10-19
**Sources:** The user's command-palette readability request and selection of option A: code-level shared style parameters with local overrides, not user-facing TOML configuration. The user also required a visibly distinguishable palette input surface similar to search.
**Related:** [ADR 0066](./0066-widget-local-backdrop-blur-with-application-glass-presets.md), [ADR 0067](./0067-widget-gradient-paints-with-application-material-presets.md), [ADR 0068](./0068-local-backdrop-edge-refraction.md), [ADR 0069](./0069-command-palette-source-aware-availability-and-focus.md).

## Context

At decision time, search composed actual local backdrop filtering with an application-owned material recipe, while the command palette reused translucent panel/input decoration without the backdrop filter. Reusing translucent paint alone did not soften the terminal detail behind the palette and could compromise readability.

The user wants one configurable appearance strategy without forcing every location to look identical. They selected code-level configuration rather than extending the user configuration contract.

## Decision

1. Search and the command palette share application-owned glass appearance defaults, with per-consumer overrides of selected style parameters; unspecified parameters retain the shared defaults. This is code-level configuration, not new TOML settings or a live-configuration protocol.
2. The palette uses the same real local background-blur strategy as search rather than only sharing translucent decoration. Reuse existing widget filtering/painting mechanisms; product recipes and their overrides remain application-owned. This extends the consumers of ADRs 0066–0068 without replacing their mechanism/ownership decisions.
3. Shared material styling does not require the input and surrounding panel to have the same surface treatment. Keep the palette editor visibly distinguishable from its panel, as in search, while its foreground remains sharp.

## Consequences

- Shared defaults can be tuned in one place while a consumer can intentionally vary selected appearance parameters without duplicating the material recipe.
- This does not authorize a new generic glass framework, changed widget-wide style defaults or expansion of the user-facing configuration schema.
- Exact numerical contrast/material values remain reversible application tuning. `harbor-app::appearance::GlassAppearance` now supplies shared defaults and a command-palette editor override, and both consumers use its composed panel wrapper. Focused tests and a bounded native visual smoke are recorded in [the appearance evidence](../../docs/verification/command-palette-glass/README.md); this is not full command-palette or cross-environment visual acceptance.

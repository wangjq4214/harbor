# Reusable Widget Vector Icons Without Font Glyphs

**Status:** Testing
**Date:** 2026-10-14

## Context

The application side rail currently paints terminal, unread, new-terminal, and close marks using text in `crates/harbor-app/src/ui.rs`. The existing `harbor-widget::IconButton` also uses a text glyph rather than an independent icon component. The user accepted the recommendation to introduce a reusable vector icon component with size and color controls, using an existing SVG outline icon collection instead of system-installed icon fonts.

This extends the generic widget visual capabilities established in [ADR 0003](./0003-widget-independent-pipeline.md). Image/path icons were deferred from the original tab delivery in [Spec 0010](../spec/0010-desktop-terminal-tabs-and-view-macro.md); this is a subsequent capability, not a claim that the original tab delivery required it.

## Decision

The reusable `Icon` component belongs in `harbor-widget`. Its artwork is vector-based and sourced from an existing SVG outline icon collection rather than painted as terminal-font text or resolved through a system-installed icon font. The application retains ownership of side-rail composition and tab behavior.

Use [Lucide](https://lucide.dev/) as the SVG outline icon collection, confirmed by the user on 2026-10-14. This is an artwork-source commitment, not a dependency on Lucide's JavaScript or React packages. Preserve the applicable upstream license notices when integrating the assets. The implementation uses a bounded precompiled SVG subset (terminal, plus, x, and dot) as signed-distance functions within the retained quad renderer. Each icon is one Primitive::Icon; a single distance union preserves opacity at overlapping strokes. This does not add an arbitrary SVG loader, image atlas, or separate platform renderer.

## Consequences

- Icon size and color can be controlled independently of terminal text metrics and font selection.
- Icon appearance does not require the user to install an icon font.
- Widget rendering must gain the capability needed to display the selected vector artwork; a text-only wrapper does not realize this decision.
- Original SVGs and upstream license notices are bundled under `crates/harbor-widget/assets/lucide/`. Adding artwork requires extending the built-in definitions and GPU pixel tests together.

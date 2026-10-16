# Widget Local Backdrop Blur with Application Glass Presets

**Status:** Testing
**Date:** 2026-10-16
**Sources:** The user's search-bar appearance request and explicit selection of option B (real local backdrop blur rather than a translucent visual approximation). The user also chose to preserve existing search navigation behavior.
**Related:** [ADR 0064](./0064-widget-and-application-component-ownership.md), [ADR 0065](./0065-layered-widget-text-input.md), [ADR 0023](./0023-windows-acrylic-backdrop-not-mica.md).

## Context

The search panel overlays terminal content. Translucent fill, rounded corners and shadows can suggest glass, but do not blur the terminal text behind the panel. Windows Acrylic is a host/compositor backdrop behind the window, not a local filter over content already drawn inside it. The user selected actual local softening of the underlying terminal content while keeping the panel's input, result text and buttons sharp.

Existing component ownership keeps reusable visual mechanisms in `harbor-widget` and product composition and appearance parameters in the application. The existing `TextField.decoration()` and `IconButton.style()` already expose rounded/translucent field decoration and state-dependent button colors; these appearance requests alone do not justify new search-specific widget interfaces.

## Decision

1. Provide real local backdrop blur as a reusable `harbor-widget` visual/rendering capability, not as search-specific rendering or a terminal-domain styling API. It must be usable without Harbor search, session or command types.
2. The search panel uses that capability to soften content behind its bounds, including the embedded terminal presentation, without blurring the panel's own input, result text or buttons. Merely lowering panel opacity or relying on Windows Acrylic is not the selected effect.
3. Keep the search panel's glass preset, layout, result wording, button appearance and input decoration application-owned. Reuse existing styling interfaces where sufficient instead of moving those product choices into widget defaults.
4. Public API names, blur algorithm, render-target/pass arrangement, parameter values and performance thresholds remain unselected. This record does not authorize a general arbitrary-filter framework.

## Consequences

- Delivery requires a widget rendering capability in addition to application styling; a translucent-card approximation alone does not fulfill the selected direction.
- The effect must compose with embedded terminal drawing without moving search semantics or terminal content ownership into the widget layer.
- This decision adds a future local-blur capability; it does not redefine existing `BoxDecoration` fields or replace the window Acrylic policy.
- This records the approved direction only, not implementation or runtime acceptance.

# Declarative GPU UI with host-owned native dialogs

> Superseded by [ADR 0007: Renderer-owned UI boundary](0007-renderer-owned-ui-boundary.md).

The prior plan to rename `harbor-render` into `harbor-gpu` and let UI runtime instances own GPU surfaces is no longer current. Harbor instead retains a domain-neutral `harbor-gpu` and introduces `harbor-render` as the sole UI GPU rendering layer.

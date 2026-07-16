# GPU runtime boundary

> Superseded by [ADR 0007: Renderer-owned UI boundary](0007-renderer-owned-ui-boundary.md).

`harbor-gpu` remains domain-neutral. The prior decision that `harbor-ui` owns terminal rendering is no longer current; `harbor-render` now owns all GPU rendering under ADR 0007.

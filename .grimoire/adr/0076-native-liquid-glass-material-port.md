# Native Liquid Glass Material Port

**Status:** Testing
**Date:** 2026-10-22
**Sources:** The user linked Glass-HQ/liquid-glass and then accepted the recommendation: search and command-palette panels first, material effects only, native Rust/wgpu rather than React/WebView, with a spec-only endpoint before implementation.
**Related:** [ADR 0066](./0066-widget-local-backdrop-blur-with-application-glass-presets.md), [ADR 0067](./0067-widget-gradient-paints-with-application-material-presets.md), [ADR 0068](./0068-local-backdrop-edge-refraction.md), [ADR 0070](./0070-shared-application-glass-presets-with-local-overrides.md).

## Context

Harbor already samples live in-window content through its widget backdrop renderer and composes application-owned glass presets. The selected reference uses WGSL to generate displacement and rim-light maps, then browser SVG filters to apply blur, displacement and material color processing to live DOM content. Its React components, DOM adapters and GPU readback are integration machinery for the web, not requirements for Harbor.

The reference inspected for this decision is [Glass-HQ/liquid-glass at a4331da89ee47bcaaef3276de522257f37b0ee26](https://github.com/Glass-HQ/liquid-glass/tree/a4331da89ee47bcaaef3276de522257f37b0ee26). Its [MIT license](https://github.com/Glass-HQ/liquid-glass/blob/a4331da89ee47bcaaef3276de522257f37b0ee26/LICENSE) permits adaptation with the required copyright and permission notice retained in copies or substantial portions.

## Decision

1. Adapt the reference's material algorithms into Harbor's native Rust/wgpu rendering path. Do not introduce React, a WebView, a browser SVG-filter runtime or the upstream GPU-to-PNG/DOM bridge to obtain the effect.
2. Extend/reuse the existing widget-local backdrop mechanism, rather than create a parallel search-specific renderer or import the upstream control toolkit. Generic sampling, edge refraction and material composition belong to `harbor-widget`; shared product recipes and per-panel overrides remain in `harbor-app`, as established by ADRs 0066–0070.
3. The selected material direction combines edge-local background displacement, outline-normal-dependent highlights and side shading, and material blur/tone processing. Foreground and hit testing remain unaffected. Static geometric highlights are not a selection of dynamic lighting or motion.
4. Sample already-drawn in-window content, including terminal presentation. The existing host Acrylic backdrop is not a sampleable desktop texture; this decision neither replaces it nor authorizes desktop capture or whole-window desktop refraction.
5. Preserve required upstream attribution when adapting its code. Exact APIs, pass arrangement, cache strategy and material tuning remain implementation choices within existing clipping, alpha, bounded-sampling and ownership contracts.

## Consequences

- Search and command-palette panels are the first selected consumers, not an application-wide restyle. Their behavior and existing geometry are preserved; new material work is governed by [spec 0032](../spec/0032-native-liquid-glass-material.md).
- Upstream press/stretch/morph/fusion animations and web control behavior are outside this delivery. Existing interactions are retained, not removed.
- Existing blur-only consumers and solid/gradient decoration APIs remain compatible. This decision supplements, rather than supersedes, the prior local-blur, edge-refraction and shared-preset boundaries.
- Native implementation has workspace tests, targeted DX12 pixel tests and bounded native smoke evidence; full cross-environment visual acceptance remains pending. Local details are in `docs/verification/native-liquid-glass-material.md` (ignored, not retained repository evidence). The subsequent user request includes notifications without changing their lifetime or interaction policy.

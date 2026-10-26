# Native Liquid Glass Material for Search, Command Palette and Notifications

**Spec ID:** 0032
**Status:** Accepted (owner-confirmed)
**Date:** 2026-10-22
**Sources:** The user linked Glass-HQ/liquid-glass, approved the search + command-palette native material-only scope, then added notifications while retaining their existing behavior. [ADR 0076](../adr/0076-native-liquid-glass-material-port.md) records the architectural direction.

## Source Contract and Relationship to Existing Specifications

- **C1 — Selected outcome:** Adapt the reference's glass material to the existing search, command-palette and notification panels, not a whole-application restyle.
- **C2 — Selected solution:** Native material algorithms over the existing widget backdrop path; reusable rendering mechanisms in `harbor-widget`, application recipes and per-panel overrides in `harbor-app`. No React/WebView/SVG runtime integration.
- **C3 — Selected effects:** Edge-local refraction, outline-normal-dependent highlights and side shading, and material blur/tone processing. Keep foreground and hit testing unchanged.
- **C4 — Selected exclusions:** No upstream press/stretch/morph/fusion animation work, desktop capture/refraction or side-rail/button restyle.
- **C5 — Retained contracts:** [Search appearance spec 0029](./0029-search-panel-glass-appearance.md), [command-palette spec 0030](./0030-command-palette.md), and [ADR 0070](../adr/0070-shared-application-glass-presets-with-local-overrides.md) remain applicable. This spec adds a shared material upgrade, not new search, editing, command or focus semantics.

The external source baseline is [Glass-HQ/liquid-glass commit a4331da89ee47bcaaef3276de522257f37b0ee26](https://github.com/Glass-HQ/liquid-glass/tree/a4331da89ee47bcaaef3276de522257f37b0ee26):

| Source | Verified role |
| --- | --- |
| [maps.wgsl](https://github.com/Glass-HQ/liquid-glass/blob/a4331da89ee47bcaaef3276de522257f37b0ee26/packages/liquid-glass/src/gpu/maps.wgsl) | Outline distance/normal, nonlinear edge displacement, coverage and bright/dark rim overlay |
| [material-graph.ts](https://github.com/Glass-HQ/liquid-glass/blob/a4331da89ee47bcaaef3276de522257f37b0ee26/packages/liquid-glass/src/dom/material-graph.ts) | Regular-material frost combination, material tone curves, chroma/tint processing |
| [materials.ts](https://github.com/Glass-HQ/liquid-glass/blob/a4331da89ee47bcaaef3276de522257f37b0ee26/packages/liquid-glass/src/core/materials.ts) | Clear/Regular and light/dark reference constants; not mandated Harbor defaults |
| [package README](https://github.com/Glass-HQ/liquid-glass/blob/a4331da89ee47bcaaef3276de522257f37b0ee26/packages/liquid-glass/README.md) | Browser architecture, material/motion separation and supported-scope limitations |
| [LICENSE](https://github.com/Glass-HQ/liquid-glass/blob/a4331da89ee47bcaaef3276de522257f37b0ee26/LICENSE) | MIT copyright and permission-notice obligations |

These references explain the target mechanisms. They do not require copying the sample page's layout, control kit, continuous-corner geometry or numerical defaults, nor do they establish pixel-exact or physical-optics acceptance.

## Requirements

### R1 — All three panels use the upgraded shared material

Search, the command palette and notifications must use the material over live in-window content, including terminal presentation. Merely increasing the existing displacement/blur or changing a translucent gradient border does not fulfill C3. Product recipes retain shared defaults with intentional local overrides, rather than duplicated independent material implementations.

Preserve each panel's allocation, position, existing corners, clipping and controls. In particular, search retains its 12-logical-pixel panel and 7-pixel editor radii, single-row layout and fixed result allocation under spec 0029. Preserve the command palette's current geometry and distinguishable inset editor under ADR 0070. Notifications retain their existing dimensions, placement, close/scroll/focus behavior and three-second auto-dismissal, including errors, under ADR 0071. The material change must not move controls, alter terminal grid allocation or introduce a new control surface.

### R2 — Live edge-local refraction and geometric rim response

Use the reference's nonlinear edge-displacement model as the basis for the upgraded refraction. Background lines/text near edges and corners must visibly bend or compress; displacement fades to zero toward the stable center. Keep sampling support bounded and scaled consistently with logical geometry/DPI. The existing safe limits need not be replaced by the reference's larger default displacement.

Rim highlights and side shading must respond to the outline normal and edge distance, not only a fixed rectangle-wide gradient. Required observations include a fine bright/dim rim with differentiated top/bottom and side response, including at rounded corners. This is a static material response: no new moving light, pointer-following illumination or surface motion is required.

Foreground glyphs, caret, selection, icons, layout and hit testing must not be refracted, shaded as background or geometrically transformed. Preserve the existing search direction of fine, translucent uneven highlights and no outer shadow; do not replace the result with an equally bright thick white outline.

### R3 — Material blur and color response, not an opaque imitation

Adapt the reference's blur/tone material processing to native GPU composition. For these panels, maintain the existing real soft-backdrop/readability contract: underlying fine terminal detail softens while broad colors and shapes remain transmitted, and panel content stays sharp. The Regular-material frost/tone source is a reference for that softening, not a promise of a new user-selectable material mode.

Tone processing must derive its result from the sampled background rather than manufacture a fixed colored patch or wallpaper. Preserve the search panel's faint neutral black veil, translucent appearance and absence of a heavy milky or opaque dark backing. Keep both panel editors visibly distinguishable and readable. Exact blur, tone, rim and tint values are reversible application tuning, not new numerical acceptance thresholds.

The upstream Clear/Regular and light/dark variants do not authorize an appearance-switching UI, new theme policy or a requirement to expose every upstream parameter. Existing terminal palette and whole-window Acrylic policies remain independent.

### R4 — Correct native composition and compatibility

Reuse/extend the current widget-local backdrop mechanism. Draw order must make already-painted content available to the material before painting the panel foreground. Embedded terminal draws must participate without replaying their state-changing presentation merely to obtain a backdrop.

Preserve premultiplied-alpha behavior, intended color response, rounded/ancestor clipping and live updates. Filtering must not double-compose sharp source pixels, darken transparent edges incorrectly or spill outside the intended region. Resource/sampling bounds remain valid for small panels, fractional DPI, resize and window-edge placement.

Ordinary blur-only consumers, effects-disabled paths, and existing solid/gradient decoration interfaces retain their behavior. Generic rendering must be usable with a non-terminal scene and contain no search, command or session semantics. The chosen work is not a comprehensive new Glass control framework or arbitrary-filter framework.

### R5 — Existing application behavior remains unchanged

Retain search query/result/navigation behavior, command-palette filtering/availability/execution, existing keyboard/IME/focus routing, clipboard authorization and input isolation. Do not replace editors or import the upstream web controls. Existing hover/press/focus feedback remains; excluding upstream motion does not remove current interaction feedback.

No new TOML configuration or changes to global widget appearance defaults are selected. Only the three selected panel recipes adopt the new material; side rail, standalone buttons and the whole native window are not automatically restyled.

### R6 — Attribution and honest evidence

Retain the upstream copyright and MIT permission notice with adapted code/substantial portions and in the delivered distribution as applicable. Document the upstream revision and adapted source scope so later maintenance can distinguish provenance from Harbor-specific changes.

Actual native visual evidence is required for material acceptance; shader compilation or passing GPU tests alone is insufficient. Record executed versus NOT RUN checks and reproducible comparison conditions.

## Solution and Integration Seams

Adapt the reference's material mathematics to GPU-native passes over the existing live backdrop. The upstream map-to-readback-to-PNG-to-SVG bridge serves browser integration and is not the selected Harbor solution. Exact API names, shader/pass partition, geometry representation and caching remain implementation choices, not an implementation plan in this contract.

| Seam | Connects | Expects | Provides |
| --- | --- | --- | --- |
| Scene/background | Widget frame encoder and embedded terminal drawing → backdrop material | Already-painted in-window pixels, scene order, panel geometry, DPI and applicable clips | Live bounded background sampling without replaying terminal side effects or sampling panel foreground |
| Material mechanism | Widget material/filter inputs → native GPU rendering | Validated geometry and bounded optical/material parameters | Edge-local displacement, geometric rim response, blur/tone composition and correct transparent coverage |
| Product composition | Application shared recipe/local overrides → widget mechanisms and panel decoration | Product-owned material choices and existing child content | Consistent material on search, palette and notifications, local editor distinction, no layout/hit-test changes |
| Platform boundary | Native window Acrylic → final desktop composition | Existing host backdrop policy; no sampleable desktop input is supplied | Unchanged host Acrylic behavior, not a claim that local shaders refract other applications |
| Provenance | Adapted upstream source → repository/distribution notices | Pinned source baseline and actual reused portions | Preserved MIT attribution and auditable origin |

## End-to-End Verification

| ID | Given / when | Required outcome |
| --- | --- | --- |
| E1 | Open search over terminal output containing high-contrast lines/text and broad colored regions; continue live output | Fine background detail softens, edge/corner content bends, broad colors transmit and update; center geometry and foreground remain stable/sharp |
| E2 | Open the command palette over the same controlled content; edit/filter and navigate results | Same shared material direction, distinguishable readable editor, existing palette geometry and operation; no terminal input leakage |
| E3 | Compare the old preset and upgraded material under the same background, geometry and DPI | Upgraded rim exhibits normal-dependent top/bottom/side/corner response and nonlinear edge refraction; improvement is not merely stronger blur, displacement or a gradient-border recolor |
| E4 | Change background from contrasting colored detail to neutral uniform, dark and light scenes | Material follows real content, with readable foreground; no fixed green/wallpaper patch, opaque backing or artificial variation demanded from a uniform source |
| E5 | Resize, change DPI including fractional scaling, and place a filtered rounded region near window/ancestor clip edges in a controlled native scene | No outside-region filtering, clipping halo, duplicate sharp background, invalid sampling or foreground/control movement; corner/rim behavior follows geometry |
| E6 | Exercise a generic non-terminal root and compare ordinary blur-only/effects-disabled/solid-paint consumers against their baseline | New material works without terminal types; existing consumers retain expected rendering and geometry |
| E7 | Use search next/previous/close and palette filtering/invocation/IME then return to terminal interaction | Existing commands, counts, focus destinations and consumed-input isolation remain intact; no upstream control/motion behavior is introduced |
| E8 | Review adapted files and a delivered package's applicable license notices | Upstream revision/reused scope is identifiable and the required MIT copyright/permission notice is present |
| E9 | Show a notification, including an error, over live terminal content; exercise close/scroll and allow another to expire | Shared upgraded material with sharp foreground; existing geometry/placement, no focus theft and three-second auto-dismissal remain unchanged; no upstream motion is added |

## Verification Guidance

- Cover edge falloff/normal response, parameter bounds, DPI, alpha/clipping and scene order with focused widget/GPU tests; use controlled image comparisons to distinguish the old gradient-border approximation from the new material.
- Cover shared/default/local recipe propagation for both consumers and unchanged panel/editor geometry. Reuse applicable search/palette behavior tests rather than interpreting a visual change as permission to revise their semantics.
- Capture native before/after evidence for E1–E5 with revision, DPI, background and panel conditions recorded. Verify live updates rather than using only a static reference screenshot. Exact visual tuning is not a pixel-perfect comparison against the upstream web demo.
- Observe render/resource cost under the same controlled conditions and retain existing scheduling/bounded-resource obligations. No new numeric frame-time, memory or visual-difference threshold was selected; do not invent one or claim performance acceptance from test existence.

## Decisions, Assumptions and Exclusions

- **Native port rather than embedding:** C2 and ADR 0076. Existing widget/app boundaries remain in force under ADRs 0066–0070.
- **Material-only delivery:** C1/C3/C4. Three consumers share one rendering contract.
- **Retained appearance constraints:** C5. No prior ADR is superseded; geometric static rim shading is not the previously excluded dynamic-lighting system. Existing search geometry/translucency/no-shadow and palette editor distinction remain requirements.
- **No unconfirmed product assumption:** Material numeric tuning and rendering internals are implementation choices within the settled contract, not new domain decisions. A need to change prior geometry, scope, appearance policy or platform sampling must return to refinement.
- **Outside scope:** Whole-window desktop refraction/capture; side-rail/button restyling; upstream press/stretch/morph/fusion animations; web control behavior; appearance/material switching UI; new TOML schema; arbitrary filters; comprehensive Glass toolkit; pixel-exact/physical-optics guarantees.

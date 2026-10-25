# Named Material Inputs Keep the Existing Glass Effect

**Ticket ID:** T0002
**Source:** [spec 0041](../../spec/0041-widget-api-naming-and-configuration-validation.md) R6, R8, and the direct-Result boundary of R5; [ADR 0087](../../adr/0087-widget-api-naming-and-validation-boundaries.md) D7; [related file](./README.md).
**Status:** Todo

## Goal

Let the public tone/rim inputs express meaning through fields while keeping the existing legal domain, caps/clamps, GPU packing, and application glass recipe effects.

## Affected Surfaces

- The material inputs in `widgets/backdrop_filter.rs` and related exports, and the renderer packing boundary that actually consumes its configuration.
- Application appearance and the search/palette/notification glass consumers, related parameters, and GPU/scene regressions.
- Public API/fixtures and the material documentation. The existing algorithm/renderer does not have to be rewritten because of naming.

## Approach

Accept the local named tone/rim inputs at the existing BackdropMaterial consumption boundary; the internal array may stay. Convenience entries reuse the same validation and keep no compatibility path for the old public positional arrays. The input type spelling is chosen within the spec behavior; do not invent new material options or defaults.

## Dependencies and Coordination

- **Blocked by:** none; does not depend on T0001's style validation implementation, since the material domain is already fixed by spec R6.
- **Blocks:** T0006 full acceptance.
- **Coordination risks:** shares appearance/notifications and the actual glass scenes with T0004; shares exports and docs with other tickets. Independent development requires coordinated file edits; integrating in README order is recommended.

## Acceptance

- [ ] The public tone input names offset/slope/quadratic/saturation and the rim input names highlight/shade; no old configuration entry requiring you to remember array positions remains.
- [ ] Every input rejects non-finite values; negative sigma, saturation, and rim are still rejected, while negative mix/coefficients remain legal. Errors identify the actual invalid field/rule and keep returning a direct Result.
- [ ] The existing caps/clamps are preserved: blur/fill sigma max 64dp, refraction width/displacement max 64/32dp, mix [-1,1], the three tone coefficients [-2,2], saturation max 2, rim max 1. A finite legal value above a cap is not a new Err.
- [ ] Named inputs and convenience entries produce the same effective values, the internal GPU parameter fields map correctly, and zero blur and optional material/refraction behavior are unchanged.
- [ ] The search/palette/notification recipe values, background sampling, clip/paint, foreground clarity, layout, and hit range keep their original semantics; product policy is not moved into the widget crate.
- [ ] Spec E5's raw-domain matrix, cap/packing, and actual scene/GPU glass results all have evidence; helper/array tests alone do not declare the actual effect passed, and GPU checks that could not run are listed explicitly.
- [ ] All real calls, fixtures, exports, and current docs are migrated, leaving no consumer that needs the old input signature to compile.

## Out of Scope

New rendering algorithms, product material tuning, removing caps, desktop capture, global restyling, the Host GPU error path, or restructuring renderer draw order.

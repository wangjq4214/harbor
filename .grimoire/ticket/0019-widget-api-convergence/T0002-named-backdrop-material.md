# Named Material Inputs Keep the Existing Glass Effect

**Ticket ID:** T0002
**Source:** [spec 0041](../../spec/0041-widget-api-naming-and-configuration-validation.md) R6, R8, and the direct-Result boundary of R5; [ADR 0087](../../adr/0087-widget-api-naming-and-validation-boundaries.md) D7; [related file](./README.md).
**Status:** Done

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

- [x] The public tone input names offset/slope/quadratic/saturation and the rim input names highlight/shade; no old configuration entry requiring you to remember array positions remains.
- [x] Every input rejects non-finite values; negative sigma, saturation, and rim are still rejected, while negative mix/coefficients remain legal. Errors identify the actual invalid field/rule and keep returning a direct Result.
- [x] The existing caps/clamps are preserved: blur/fill sigma max 64dp, refraction width/displacement max 64/32dp, mix [-1,1], the three tone coefficients [-2,2], saturation max 2, rim max 1. A finite legal value above a cap is not a new Err.
- [x] Named inputs and convenience entries produce the same effective values, the internal GPU parameter fields map correctly, and zero blur and optional material/refraction behavior are unchanged.
- [x] The search/palette/notification recipe values, background sampling, clip/paint, foreground clarity, layout, and hit range keep their original semantics; product policy is not moved into the widget crate.
- [x] Spec E5's raw-domain matrix, cap/packing, and actual scene/GPU glass results all have evidence; helper/array tests alone do not declare the actual effect passed, and GPU checks that could not run are listed explicitly.
- [x] All real calls, fixtures, exports, and current docs are migrated, leaving no consumer that needs the old input signature to compile.

## Out of Scope

New rendering algorithms, product material tuning, removing caps, desktop capture, global restyling, the Host GPU error path, or restructuring renderer draw order.

## Implementation and Verification Evidence

Implemented against baseline `f30b15ffd2e3e755cd766c99c016f57cf8349219`; all checks below target the combined working tree, including the new public integration test. Plan: `.grimoire/plans/0052-named-backdrop-material.md` (local ignored planning artifact).

- Public `BackdropTone` and `BackdropRim` are consumed only through the existing direct-Result `BackdropMaterial::new` boundary. No array-input compatibility path, new defaults, or convenience constructors were added. Compact read-only effective-value accessors remain.
- `crates/harbor-widget/tests/backdrop_material.rs` covers every material field's NaN/±infinity, signed/nonnegative domains, zero, extreme finite values, caps/clamps and post-consumption raw-input mutation, with exact field/rule diagnostics. Blur/refraction domains are also covered.
- Renderer `Region::new` tests assert distinct named tone/rim mapping and capped GPU parameters at 1×/1.5×/2× DPI. Production packing, shaders, widget paint/layout/hit logic and scene order are unchanged.
- Application appearance tests pin the original panel/row numbers. Existing mounted search/palette/notification tests cover shared material consumption, clipping, foreground ordering, geometry, close hit behavior and lifetime. Recipes remain application-owned.
- Current material docs and all real callers/GPU fixtures use the named signature. The material doctest demonstrates recoverable construction; its compile-fail example rejects the old positional signature.

### Executed checks

| Check | Result |
| --- | --- |
| `HARBOR_TEST_BACKEND=dx12 cargo test -p harbor-widget -p harbor-app --lib --tests` | 1263 passed, 4 ignored; 3 ignored app GPU tests executed explicitly below, 1 native-window smoke not run |
| `HARBOR_TEST_BACKEND=dx12 cargo test -p harbor-widget --test quad_renderer backdrop_ -- --nocapture` | 9 passed both before and after migration; actual DX12 adapter confirmed, no silent GPU skips |
| `HARBOR_TEST_BACKEND=dx12 cargo test -p harbor-app --lib rendered -- --ignored --nocapture` | 3 passed: actual search, palette and notification pixels, 1×/1.5×/2× DPI, BGRA unorm and sRGB; notification checks locality, rounded cutout, visible rim and retained-frame equality |
| `cargo test -p harbor-widget --doc` | Passed, including named construction and old-array compile rejection |
| `cargo check --workspace --all-targets` | Passed; existing unrelated unused-variable warnings remain |
| Recipe-module doctests through `target/docs-examples/Cargo.toml` | All 4 recipe examples passed, including the named material example |
| Scoped `rustfmt --check` and `git diff --check` | Passed |

GPU evidence used AMD Radeon RX 7900 XT, DX12, driver 32.0.32015.7029. Other GPU backends/hardware, native-window visual inspection, native focus/IME and real HMR generation teardown were not run; these results do not establish full U4/T0006 or cross-environment visual acceptance.

### Review, assessment and known baseline issues

Inline review inspected the complete diff and new test plus material consumers; no confirmed change-induced blockers. All seven T0002 criteria match the scoped code/runtime evidence. Final integrated checks were refreshed after formatting.

Repository-wide Markdown examples remain 15 passed / 1 failed: unchanged `docs/widget/catalog.md:106` uses `?` outside a Result-returning context. Repository docs validation also fails on existing Han characters in unchanged `UI_API_REVIEW.md`. Neither failure involves this ticket's API/example; both were left untouched rather than broadening scope. Raw logs are local under `target/t0002-*.log`; the scoped evidence above is retained here.

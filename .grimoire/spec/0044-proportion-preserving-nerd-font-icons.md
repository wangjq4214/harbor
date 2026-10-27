# Natural-Size Nerd Font Icons with Natural Overflow

**Spec ID:** 0044
**Status:** Implemented — focused automated/native verification recorded below; broader application/font acceptance not established
**Date:** 2026-10-27
**Sources:** User request to stop clipping, accept overlap and let icons overflow naturally, followed by approval to revise this spec; [ADR 0093](../adr/0093-natural-overflow-nerd-font-icons.md), superseding [ADR 0092](../adr/0092-natural-size-clipped-nerd-font-icons.md) and the earlier fit policy in [ADR 0091](../adr/0091-proportion-preserving-nerd-font-icons.md).
**Endpoint:** Initially spec-only refinement; subsequently authorized by explicit grimoire-loop invocation for direct implementation and targeted verification, without a ticket set or separate implementation plan.

## Requirements

### R1 — Preserve natural size, proportions and artwork

Apply this policy to the official Nerd Fonts icon collection, not every private-use Unicode character or oversized ordinary glyph. Render icon artwork at its natural size for the current font, font size and DPI, preserving its proportions and natural bearings. Do not horizontally squeeze, uniformly shrink-to-fit or arbitrarily enlarge it. Do not crop the artwork to its assigned cells or displayed-row height: natural overhang is allowed, subject only to the overall boundary in R2.

Source: the user's natural-overflow request and ADR 0093; the official-icon scope and current-font sizing remain unchanged.

### R2 — Allow overflow and overlap instead of borrowing-dependent clipping

Icon artwork may extend beyond its logical cells even when it overlaps neighboring content. Do not compute an icon clipping boundary from contiguous following blanks or stop the artwork at actual text, special/styled or different-background blanks, hyperlinked/protected blanks, hidden/combining content or wide-character continuation cells. Cursor, selection and IME preedit occupancy likewise do not impose an icon-specific clipping boundary. These text units and overlays retain their existing meaning; they are not reclassified as empty or erased to make room.

At fixed font, size and DPI, changing neighboring content or overlay occupancy must not shrink or crop the icon to protect that content. Overlap and its readability tradeoff are accepted. This replaces both the previous blank-borrowing eligibility rules and the previous guarantee that icons never cover neighboring text.

**Boundary assumption retained from refinement:** Existing clipping at the overall terminal content viewport/window remains. Natural overflow does not authorize drawing outside that viewport or over unrelated application UI. An icon at the terminal's outer edge may therefore still be cut by that outer boundary. This is not per-cell, per-row or neighbor-dependent clipping.

Source: the user's explicit acceptance of overlap, ADR 0093 and the stated outer-boundary assumption accompanying the approved spec route.

### R3 — Keep logical text and geometry unchanged

Artwork overflow is not extra logical column allocation. Do not insert/remove spaces, rewrite source text, move or compress neighboring text, change model-assigned width or cursor advancement, or change wrapping, reflow and selection/copy semantics to accommodate it. Hyperlink and protection metadata remain unchanged; visual overlap does not transfer those properties between cells.

The shared text-unit/grid contract of [ADR 0044](../adr/0044-unicode-text-unit-and-presentation-width-policy.md) remains authoritative. This narrowly qualifies [Spec 0015](0015-unicode-terminal-text-correctness.md)'s assigned-cell presentation bounds for Nerd-icon artwork, not its logical-width policy. Ordinary text and emoji presentation policies are unchanged.

Source: the unchanged presentation-only constraints carried forward by ADR 0093 and the established Unicode contract.

### R4 — Keep overflow coherent across updates

Neighbor-only and overlay-only updates, including cursor movement, selection and IME changes without new PTY output, must preserve R1–R2 rather than reinstate the old occupancy-dependent clipping. When an icon changes, moves or is erased, previously drawn overflow must not remain as stale pixels. Updates to neighboring content and overlays must remain coherent with the current scene.

Font, DPI and viewport changes must use current natural glyph metrics and geometry without changing R3. A viewport change alone must not introduce shrink-to-fit; only the existing overall viewport boundary limits the visible extent.

Source: ADR 0093's natural-size/overlap policy and the existing coherent-update and font/DPI requirements of this spec, adapted from the superseded clipping contract.

## Solution and Integration

Retain the existing terminal model and font/raster/renderer ownership boundaries. Keep logical column allocation separate from visible ink bounds, present the natural-size icon artwork with its bearings, and do not restrict it through assigned-cell or following-blank regions. Preserve the existing overall terminal viewport clipping. This contract does not choose an atlas format, shader, cache structure, invalidation algorithm or new global paint-order policy; ordinary compositing is distinct from icon-specific geometry/UV cropping.

| Seam | Exchange | Guarantee |
| --- | --- | --- |
| Terminal state and overlays → renderer | Retained text units/widths, style and metadata, current overlays and coherent updates | Logical data remain unchanged; neighboring occupancy does not define an icon-specific fit/clip region. |
| Font/raster data → icon presentation | Natural glyph dimensions/bearings and current font/DPI metrics | Natural size and proportions are preserved independently of available following blanks. Fallback metrics do not redefine logical columns. |
| Viewport → rendered result | Current terminal content allocation and outer clipping | Existing overall clipping remains; individual cell/row bounds do not crop icon artwork within it. |
| Rendered result → selection/copy/cursor behavior | Same source content and model coordinates | Overflow neither moves text nor adds copied spaces/columns or logical cursor movement. |

Source surfaces: [glyph presentation](../../crates/harbor-terminal/src/render/text.rs), [current Nerd-icon presentation](../../crates/harbor-terminal/src/render/nerd_icons.rs), [cell and snapshot contract](../../crates/harbor-terminal/src/model.rs), [native font contract](0003-windows-system-native-font-backend.md), [GPU-independent core](../adr/0045-gpu-independent-terminal-core-boundary.md). These locate integration contracts, not new implementation instructions or evidence of delivery.

## Representative End-to-End Acceptance

Use an identified installed Nerd Font and an official icon whose natural ink exceeds its logical cells. Record the font, icon/codepoint, official collection reference and rendering environment. Compare at fixed font/size/DPI away from the outer viewport edge so per-cell cropping can be distinguished from legitimate outer clipping.

| Input/change | Required visible and logical outcome |
| --- | --- |
| Icon followed by enough ordinary spaces | Natural-size artwork extends outside its logical cells without fitting or cropping to them. |
| Same icon immediately followed by text, or too few spaces before text | Same natural dimensions and uncropped icon geometry/UV extent as the sufficient-space case; overlap is permitted and text coordinates/content do not move. |
| Following styled/different-background, hyperlinked/protected or hidden/combining/wide-continuation content | No new icon clipping boundary at that content; its logical text, style and metadata remain intact. |
| Move cursor, selection or IME preedit onto/off following cells without PTY output | No occupancy-driven cropping or rescaling of the icon; overlays keep their existing logical positions and content. Assess final compositing separately from geometric clipping. |
| Write neighboring text or styled blanks and erase back to ordinary blanks, without changing the icon | Natural icon geometry remains independent of blank eligibility; current neighbors redraw coherently, without reverting to the old clipping policy. |
| Icon with natural bearings or ink extending beyond the source cell's left/top/bottom, away from the outer viewport edge | Preserve natural placement and full ink extent rather than cropping at source-cell or displayed-row bounds; logical rows and wrapping do not change. |
| Icon at the terminal's outer edge | Natural size is retained; existing overall viewport clipping applies, without painting into unrelated UI or allocating another logical row/column. |
| Change, move or erase an overflowing icon | Old artwork, including its overhang, leaves no stale pixels; surrounding current content is restored coherently. |
| Compare official Nerd Fonts icon and private-use character outside that collection | Only collection membership selects this icon policy; private-use membership alone does not. Ordinary text/emoji retain their established presentation. |
| Copy/select and observe cursor/wrapping; repeat after font/DPI/viewport changes | Original text and logical geometry remain unchanged. Font/DPI changes use new natural metrics; viewport changes alone do not fit the artwork to cells. |

Geometry/renderer checks must distinguish full natural-size artwork from the previous UV/geometry cropping, uniform shrinking and horizontal squeezing. Exercise neighbor-only/overlay-only updates and removal of overflowing artwork. Native rendered evidence is needed for appearance; logical/copy assertions alone do not establish visual correctness. Existing clipping tests are historical evidence, not passing acceptance for these revised outcomes.

## Decisions and Completion

- **Selected tradeoff:** Natural overflow with overlap replaces natural-size clipping and no-overlap protection. Source: current user request and ADR 0093. No Windows Terminal equivalence is required.
- **Unchanged constraints:** Official Nerd Fonts scope, current-font natural sizing, logical text/grid semantics, and ordinary text/emoji policies.
- **Explicit assumption:** Retain overall terminal viewport/window clipping as stated during refinement; do not remove application-level boundaries.
- **Not selected:** A new global layer priority, renderer architecture, atlas/cache layout or invalidation algorithm. This spec does not require icons to be forced above every overlay.
- **Execution route:** The initial refinement was spec-only. The user subsequently invoked grimoire-loop to implement this bounded contract directly, with focused tests and workspace compile verification; no plan or ticket artifact is needed. No performance threshold or universal font-support claim is introduced.

Natural overflow is implemented with the focused evidence below. Neither the earlier clipping results nor the new single-font fixture establish universal font appearance or reproduce the user's original workload. Broader full-application acceptance is not claimed.

## Natural-Overflow Implementation and Verification (2026-10-27)

The Nerd-icon path now emits full native-size artwork with unchanged bearings and complete atlas UVs. It no longer derives contiguous blank widths, tracks selection/cursor/preedit occupancy for clipping, or crops to cell/row bounds. Emoji retains its existing fitter and ordinary glyphs retain their existing policy. Existing host viewport scissoring and logical terminal data are unchanged.

- Source: working tree based on `d60497b5cd0b09ac0e92332167a384f2e275e1ac`, including the pre-existing uncommitted icon work. Presentation: `render/text.rs`, `render/nerd_icons.rs`; regressions: `render/nerd_icons/tests.rs`, `render/nerd_icon_tests.rs`. The superseded icon selection setter and its pipeline plumbing were removed.
- The revised native regression first FAILED against the clipping implementation: a 19px-wide icon was cut to 12px when the cursor occupied its following cell. It now passes with complete 19×15 artwork and unchanged UVs. A synthetic-edge-to-engine transition in the test was corrected to restore a full snapshot before applying engine-only incremental damage.
- `cargo test -p harbor-terminal`: PASS, 1,102 unit tests, 89 integration tests and 2 doctests; 10 ignored by default. The native icon regression was run explicitly, not counted as passing merely because it is ignored.
- `cargo test -p harbor-terminal --lib nerd_icons -- --include-ignored --nocapture`: PASS, 4 tests including the native GPU regression. Windows 10.0.26300.9711; DirectWrite; AMD Radeon RX 7900 XT / Vulkan; wgpu RGBA8 readback; FiraMono Nerd Font 20px; Nerd Fonts v3.4.0 nf-custom-folder U+E5FF; cell 12×24, native ink 19×15, bearings (0, -1). Font replacement at 16px is also exercised.
- Coverage: negative bearings/full UVs/empty and small ink; exact official membership; immediate text and narrow cells; styled source and ordinary/styled/hyperlinked/protected/hidden/combining neighbors; cursor/selection/IME-only updates; row-edge geometry; visible overhang versus outer scissoring; incremental icon erasure/movement/restoration; font/DPI/viewport changes; unchanged logical source/copy/wrapping.
- `cargo check --workspace --all-targets`: PASS. `cargo clippy -p harbor-terminal --lib -- -D warnings`: PASS. Scoped Rust formatting and `git diff --check`: PASS.
- GPU captures are local generated evidence under `target/nerd-icon-evidence-overflow/`, not checked-in artifacts. The capture environment variable must use an absolute path when run through Cargo because tests execute relative to the crate directory.
- Limits: full workspace tests, strict all-target/all-feature Clippy and full application/UI acceptance were not run for this revision. The retained historical Clippy limitation below is not a fresh gate result. One font/icon fixture does not establish all-font appearance or reproduce the user's original workload.

## Implementation and Verification (2026-10-26)

**Historical evidence — superseded clipping policy only.** These recorded results predate the natural-overflow revision and have not been rerun for it. Their PASS statuses do not establish R1–R4 above.

The Nerd-icon path uses native-size, bearing-preserving clipping with matching UV cropping. Emoji retains its existing uniform fitter; the borrowing classification and terminal model are unchanged. Geometry regressions cover tight width/height, negative bearings, restored space, small glyphs and empty/outside regions. The native retained-renderer regression covers ordinary/styled/hyperlinked/protected/hidden/combining blockers, row edges, cursor/selection/IME-only updates, incremental restoration, font/DPI/viewport changes and logical text/copy invariants.

- Source: working tree based on `d60497b5cd0b09ac0e92332167a384f2e275e1ac`, including the pre-existing uncommitted Nerd-icon implementation. Changed presentation is in `render/text.rs` and `render/nerd_icons.rs`; regression files are `render/nerd_icons/tests.rs` and `render/nerd_icon_tests.rs`.
- `cargo test -p harbor-terminal`: PASS, 1,106 unit tests, 89 integration tests and 2 doctests; 10 tests ignored by default. The icon native regression was additionally run explicitly, not counted as passed merely because it was ignored.
- `cargo test -p harbor-terminal --lib nerd_icons_native_clipping_and_retained_overlay_neighbor_updates -- --ignored --nocapture`: PASS. Windows 10.0.26300.9711, DirectWrite, AMD Radeon RX 7900 XT / Vulkan, wgpu RGBA8 readback; FiraMono Nerd Font, 20px, Nerd Fonts v3.4.0 nf-custom-folder U+E5FF. Cell 12×24, native ink 19×15, bearings (0, -1). Blocked artwork retains native sampling density and the unchanged portion's pixels; it is clipped rather than shrunk. The test also reloads the font at 16px.
- Optional captures were generated under `target/nerd-icon-evidence-clipping/`; `comparison.png` shows natural, cursor-blocked, immediate-text, one-blank and constrained-viewport cases from top to bottom. These are local generated evidence, not durable checked-in artifacts.
- `cargo check --workspace --all-targets` and strict production-library Clippy: PASS. Strict all-target/all-feature terminal Clippy is BLOCKED by 13 existing `chunks_exact_to_as_chunks` diagnostics in unchanged test files; rerunning with only that lint allowed passes, but is not a strict-gate pass.
- Formatting is scoped to the changed Rust files. Full workspace tests and full application/UI acceptance were not run; one native font/icon fixture does not establish universal appearance or reproduce the user's original workload. No overall runtime acceptance is claimed.

## Out of Scope

- Automatically assigning Nerd icons two logical columns, shifting neighboring text, or changing copied content.
- Extending the policy to every oversized ordinary text glyph without a separate decision.
- Changing [color emoji fitting](../adr/0054-color-emoji-offscreen-presentation.md) or [widget vector icons](../adr/0057-widget-vector-icons-without-font-glyphs.md).
- A ticket set or separate implementation-plan artifact for this bounded change.

# Color Emoji and Whole Sequence Presentation

**Source:** [Spec 0020](../../spec/0020-color-emoji-sequence-presentation.md), [ADR 0054](../../adr/0054-color-emoji-offscreen-presentation.md), [issue #181](https://github.com/wangjq4214/harbor/issues/181)
**Ticket folder:** `.grimoire/ticket/0015-color-emoji-sequence-presentation/`

## Overview

Deliver complete supported color/ZWJ emoji presentation without changing retained Unicode, model-assigned width, copy or neighboring cells. Use DirectWrite whole-unit shaping and reused Direct2D offscreen rasterization, then transfer tiles to a separate bounded lazy color atlas while retaining the ordinary R8 fast path. Uniformly downscale oversized artwork, position it inside assigned cells and finally clip. Unsupported presentation stays visible and explicitly distinct from complete sequence rendering.

The architecture and fit policy were confirmed in refinement. These tickets are Todo, not implementation or Windows support claims. They do not authorize general shaping, new width/copy semantics, shared-texture interoperation or resolution of Spec 0015's historical no-wrap discrepancy.

## Delivery Surfaces

- `harbor-text`: sequence requests, font-session/fallback identities, Windows DirectWrite/Direct2D resources, presentation classification and bounded native caches; preserve existing scalar consumers.
- `harbor-terminal::render`: bounded CPU/GPU color atlas, pixel/blend contract, fitted/clipped geometry, full/incremental updates and projection invalidation; preserve the GPU-independent engine.
- Actual host font/DPI/appearance transitions in the terminal renderer/application bridge; no new settings hot-reload feature is required.
- Focused backend/atlas/geometry/damage tests, Windows runtime and comparable performance evidence under `docs/verification/`, and truthful support documentation.

## Dependencies

| Producer | Consumer | Why consumer cannot complete first |
| --- | --- | --- |
| T0001 | T0002 | The renderer must consume the actual backend presentation/tile contract and unsupported outcomes to verify end-to-end composition and invalidation. Synthetic geometry tests can be prepared earlier. |
| T0002 | T0003 | Final Windows/clipboard and after-change performance acceptance must exercise the integrated renderer, caches and host transitions. Baseline capture and procedure preparation can start before integration. |

There is no blocking edge on #153's historical evidence gap or #176's remaining acceptance ticket: their existing text/width and engine/update contracts are invariants to preserve, not work newly assigned here.

## Coordination

| Tickets | Risk | Strategy |
| --- | --- | --- |
| T0001, T0002 | Shared request identity, raster bounds, format/color-space/alpha and generation semantics | Use Spec 0020's seams; settle private interface details together without changing architecture or acceptance. Sequence edits to shared `harbor-text` contracts. |
| T0001, T0002 | Shared scalar atlas APIs are also used by widget text | Preserve scalar compatibility; do not expand widget emoji scope as a side effect. |
| T0002, T0003 | A late renderer change invalidates evidence or makes a baseline incomparable | Capture the ordinary-text baseline before fast-path edits; record revisions, dirty-tree scope and identical scenarios for final comparison. |
| All | Capability or font assumptions mistaken for support | Record actual formats, native capability and font/OS versions. Return a material new semantic/architecture choice to refinement rather than inventing it during execution. |

## Recommended Order

1. Capture the simple monospace baseline before changing its path; prepare Windows fixtures/procedures from T0003 without claiming that ticket complete.
2. T0001 delivers the independently testable native presentation API and bounded resources.
3. T0002 integrates that result with the terminal atlas, geometry and resource-change/damage paths as one coherent unit.
4. T0003 completes integrated Windows, clipboard, performance and quality-gate evidence.

Backend and synthetic renderer test preparation can overlap with coordinated interfaces. Shared files alone are merge risks, not additional semantic blockers. No separate global atlas refactor or implementation-plan stage is required by this set.

## Requirement Coverage

| Spec requirement | Responsible ticket(s) | Acceptance coverage |
| --- | --- | --- |
| R1: Complete supported sequences and presentation intent | T0001, T0002, T0003 | Whole-unit native fixtures, renderer integration and named Windows visual evidence |
| R2: Text/width/copy/neighbor authority and ownership | T0002, T0003 | Model-independent projection, clipping and exact clipboard codepoints |
| R3: Native color composition and explicit fallback | T0001, T0002, T0003 | Capability-scoped tiles, alpha/foreground composition and visible supported/unsupported cases |
| R4: Identity, invalidation and resource bounds | T0001, T0002 | Bounded native/negative caches, bounded lazy atlas, generation changes and repack tests |
| R5: Fragmented updates and compatibility | T0002, T0003 | Full/incremental convergence, old-pixel replacement, overlays and runtime fragmentation evidence |
| R6: Ordinary fast path and performance | T0001, T0002, T0003 | Scalar path compatibility, ordinary-only behavior and comparable before/after costs |
| R7: Focused/Windows evidence and gates | T0001, T0002, T0003 | Focused tests at each implementation boundary and final self-contained verification |

## Ticket Index

| Ticket | File | Outcome |
| --- | --- | --- |
| T0001 | [T0001-native-emoji-sequence-presentation.md](./T0001-native-emoji-sequence-presentation.md) | Complete sequence-aware native presentation and bounded raster/cache resources are independently verifiable. |
| T0002 | [T0002-terminal-color-atlas-and-invalidation.md](./T0002-terminal-color-atlas-and-invalidation.md) | Terminal color drawing, fit/fallback and dirty/resource transitions work together without changing model/copy semantics. |
| T0003 | [T0003-windows-and-performance-acceptance.md](./T0003-windows-and-performance-acceptance.md) | Named supported/unsupported Windows results, clipboard correctness, comparable costs and quality gates are honestly evidenced. |

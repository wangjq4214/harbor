# Mutation Revisions and Incremental Terminal Updates

**Status:** Testing
**Date:** 2026-10-27
**Sources:** Refinement conversation on 2026-10-27: the owner proposed mutation revisions instead of snapshot comparison, then selected both acknowledgement comparison removal and removal of unconditional full-grid cloning during update reads.

## Context

`Terminal::read_update` currently constructs an owned `TerminalSnapshot`, cloning every visible cell. `Terminal::acknowledge_update` calls `Screen::matches_snapshot`, which compares visible cells and other snapshot state. Avoiding another snapshot construction during acknowledgement removes a clone but leaves the full-grid equality traversal when cells match. The owner selected addressing both costs rather than only the acknowledgement path.

[ADR 0045](./0045-gpu-independent-terminal-core-boundary.md) and [spec 0016](../spec/0016-terminal-engine-wgpu-renderer-boundary.md) retain the GPU-independent engine, coherent read-only renderer updates, and explicit damage acknowledgement contract.

## Decision

Use mutation-revision validation instead of full snapshot content equality for acknowledgement, and make the renderer update-read path incremental or reusable rather than unconditionally cloning the entire visible grid. Mutation revisions establish update validity; they do not replace the coherent content projection needed by the renderer. This selects the architectural direction, not a particular snapshot storage layout or public API.

Owner-confirmed clarification (2026-10-27): an intervening relevant mutation invalidates an old acknowledgement even when content changes A → B → A. Initial projection, resize and invalidation reconstruction may perform full-grid work; ordinary unchanged or local-change updates must not unconditionally copy the whole grid. Moving that unconditional copy to the mutation path does not satisfy the selected direction.

Retain the existing ownership and consumption boundary: reading an update does not consume damage; skipped or failed preparation must leave changes available for replay or reconstruction. This refines update mechanics within ADR 0045 rather than superseding its engine/renderer boundary.

## Consequences

- Removing acknowledgement-time cloning alone is insufficient: both the equality traversal and unconditional update-read grid cloning are in scope.
- Update validity must account for relevant non-cell state as well as cell content; the existing projection acknowledgement/invalidation epoch alone does not establish mutation validity.
- Renderer consumers still need a coherent state projection. Revision validation is not permission to acknowledge data that the renderer has not safely prepared.
- The architectural decision alone is not implementation or performance evidence; the implementation evidence section below records delivered work and remaining verification limits.

## Implementation evidence — 2026-10-27

The selected direction is implemented with row-shared snapshot cells and mutation provenance at state owners, separate from semantic content revisions and projection epochs. A private shared snapshot reference seals the acknowledged projection identity. Ordinary updates copy only changed row payloads even while old updates survive; full reconstruction remains available. Nonsemantic overflow recognition invalidates its projected row without invalidating independent content readers.

[Verification](../../docs/developer/incremental-terminal-updates.md) records passing core/rendered/workspace and native Vulkan pipeline checks, copy-count assertions and before/after CPU measurements. Strict Clippy and full interactive application/ConPTY acceptance remain outstanding as described there; this is not complete acceptance.

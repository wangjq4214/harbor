# GPU-Independent Terminal Core Boundary

**Status:** Proposed
**Date:** 2026-09-27

## Context

Issue #176 requires terminal state, parsing, input, selection, damage, timing, and PTY lifecycle to build and test without wgpu while preserving the existing rendered facade and widget integration. Today `Terminal` owns `TerminalRenderPipeline` and its `render` and `frame_demand` paths cross the engine/render boundary. Alternatives are separating crates immediately, isolating renderer dependencies inside the existing crate first, or retaining the coupled terminal.

## Decision

First attempt a feature-isolated core in `harbor-terminal`, verified by a core-only dependency/build target with no wgpu, arboard, winit, or `harbor-widget`; split a core crate only if feature isolation cannot prove that dependency boundary reliably. The logical engine owns terminal state and timing, while a session adapter owns PTY endpoints, reader, resize coordination, and teardown. Each session's concrete wgpu renderer consumes a coherent snapshot/damage update without mutating parser or screen state. Reading a snapshot does not acknowledge damage: changes accumulated while drawing is skipped or fails must remain renderable when drawing resumes. Keep an application-facing compatibility facade only while existing callers migrate; after migration it may be removed. Do not add multi-renderer subscription semantics or a universal renderer trait before a demonstrated need.

## Consequences

- Core-only compilation and tests, not merely a headless constructor, become the boundary proof.
- The update contract must define the point at which a renderer has safely consumed changes and when resize, viewport, or palette changes require a full upload; timing and cursor-blink ownership must not force core to depend on GPU resources. A hidden tab or failed draw must not lose pending visual changes.
- The compatibility facade protects callers during migration but is not a required permanent API; removal is allowed once all callers have migrated and lifecycle behavior is verified.
- GPU initialization failure must leave PTY endpoints safely owned or torn down; migration must preserve resize, selection, preedit, scheduling, and retained rendering.
- Existing descriptions of `Terminal` as an integrated wgpu engine remain historical descriptions of the implemented system until migration; update current context and documentation when the new boundary is delivered.

Related: [terminal GPU injection](./0011-terminal-custompaint-gpu-injection.md), [terminal render consolidation spec](../spec/0002-terminal-render-consolidation.md), [issue #176](https://github.com/wangjq4214/harbor/issues/176).

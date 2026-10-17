# Terminal Render Consolidation

**Spec ID:** 0002
**Status:** Implemented
**Date:** 2025-07-28

**Architecture evolution:** The consolidation remains current; later boundary changes are defined by Specs 0005 and 0012 and ADR 0031.
## Requirement

Merge `harbor-render` GPU rendering into `harbor-terminal`, making the terminal a self-contained engine (state + parsing + wgpu rendering + PTY I/O) that renders into the widget tree via the existing `CustomPaint` escape hatch, with zero orchestration in the app layer. Delete `harbor-render` and `harbor-ui` crates. Minimize `harbor-parser`'s public API to three items (`VtHandler`, `Params`, `Parser`).

## Solution

### Architecture

```
harbor-parser (VtHandler + Params + Parser)
     └─► harbor-terminal (Screen + wgpu rendering + PTY I/O)
           └─► CustomPaint widget (harbor-widget) ──► app (single root assignment)
```

`harbor-terminal` owns the screen grid, the VT parser bridge, all wgpu rendering pipelines (glyph atlas, cursor quad, scrollbar, selection highlight, background, decoration), and an internal I/O thread for PTY read. It does NOT own `GpuContext`, `wgpu::Device`, or a window — those are injected at render time by `CustomPaint`.

`CustomPaint` wraps a `Terminal`. During `build()`, it registers the terminal's `ExternalDrawFn` into `BuildCx`. `Runtime` stores a `HashMap<ExternalDrawId, Box<ExternalDrawFn>>` and invokes the correct handler when it encounters `Primitive::External` during `encode()`. No per-frame callback parameter is needed; `encode()` signature simplifies.

## Decisions

[ADR 0010](../adr/0010-parser-minimal-public-api.md) fixes the three-item parser API; [ADR 0011](../adr/0011-terminal-custompaint-gpu-injection.md) defines build-time external registration and GPU injection; [ADR 0013](../adr/0013-synchronous-pty-io.md) defines blocking std::io handles. Historically, App drains deferred input while Terminal interprets it, replacing [ADR 0006](../adr/0006-custom-paint-input-provider.md).

The former public harbor-render Component prepare/draw/resize abstraction dissolves into internal render components ([ADR 0012](../adr/0012-consolidate-render-ui-into-terminal.md)). The no-separate-window paste test below conflicts with Spec 0001 and ADRs 0007/0009; that discrepancy is unresolved, not silently removed.

PTY input/output accept Read/Write + Send + 'static. CustomPaint registers ExternalDrawFn by ExternalDrawId through BuildCx; Runtime invokes (id, rect, &mut RenderPass) for that allocation. Widget UiEvent input drains through drain_external_input and frame-scoped GPU access is injected, not owned.

## Test Plan

Terminal/Parser integration must render red Hello from SGR output, deliver a focused A key as the existing a byte encoding, resize 80×24 without losing scrollback, and isolate distinct draw IDs by allocation/hit test.

- **Integration tests:**
  - `harbor-terminal`: bootstrap Terminal with mock `Read`/`Write` handles; feed bytes, verify screen state; send keystrokes, verify bytes appear on write handle.
  - `harbor-widget`: verify CustomPaint registers handler via BuildCx; verify Runtime invokes correct handler for Primitive::External; verify multiple CustomPaint widgets with distinct IDs route correctly.
  - Cross-crate: Terminal + Parser integration with incremental byte feed and screen snapshot comparison.
- **Manual tests:**
  - Full PTY session: launch shell, observe rendering, type commands, verify output.
  - Window resize: drag corner, verify grid re-allocation and scrollback.
  - Paste multi-line text, verify confirmation dialog appears as widget (not separate window).
- **Performance thresholds:**
  - Parse + screen update < 500µs per typical 4KB PTY chunk.
  - GPU upload: dirty-row incremental path used for ≥90% of frames during interactive use.
  - Frame encode (wgpu draw calls) < 2ms for 80×24 grid.
- **Edge cases:**
  - PTY read returns 0 bytes (EOF) — Terminal shuts down reader thread.
  - PTY write pipe full — write blocks; callers on main thread must tolerate brief blocking.
  - Alt-screen switch mid-batch — parser returns partial consume; Terminal handles the split.
  - Font fallback for missing glyphs — Text component renders replacement character.
  - Zero-size window — Terminal skips render, does not crash.

## Out of Scope

- Async PTY I/O. Terminal uses blocking `Read`/`Write` with an internal thread.
- Multi-window support beyond widget tree split panes.
- GPU backend abstraction (wgpu is the only backend; `backend-dx12`/`backend-vulkan` features from harbor-render are removed).
- Font loading — stays in `harbor-text`, which `harbor-terminal` depends on.
- Parser feature negotiation (e.g., SGR sequences not yet implemented). Parser emits actions; Screen handler implements or ignores.
- `harbor-pty` crate — may be simplified or kept as a PTY-spawning helper; refactoring it is out of scope.
- Widget-based paste confirmation dialog implementation details — only the architectural decision (no separate `harbor-ui`) is in scope.

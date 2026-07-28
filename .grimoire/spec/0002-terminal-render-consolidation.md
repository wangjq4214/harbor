# Terminal Render Consolidation

**Spec ID:** 0002
**Status:** In Progress
**Date:** 2025-07-28

## Requirement

Merge `harbor-render` GPU rendering into `harbor-terminal`, making the terminal a self-contained engine (state + parsing + wgpu rendering + PTY I/O) that renders into the widget tree via the existing `CustomPaint` escape hatch, with zero orchestration in the app layer. Delete `harbor-render` and `harbor-ui` crates. Minimize `harbor-parser`'s public API to three items (`VtHandler`, `Params`, `Parser`).

## Solution

### Architecture

```
harbor-parser (VtHandler + Params + Parser)
     └─► harbor-terminal (Screen + wgpu rendering + PTY I/O)
           └─► CustomPaint widget (harbor-widget) ──► app (一行设根)
```

`harbor-terminal` owns the screen grid, the VT parser bridge, all wgpu rendering pipelines (glyph atlas, cursor quad, scrollbar, selection highlight, background, decoration), and an internal I/O thread for PTY read. It does NOT own `GpuContext`, `wgpu::Device`, or a window — those are injected at render time by `CustomPaint`.

`CustomPaint` wraps a `Terminal`. During `build()`, it registers the terminal's `ExternalDrawFn` into `BuildCx`. `Runtime` stores a `HashMap<ExternalDrawId, Box<ExternalDrawFn>>` and invokes the correct handler when it encounters `Primitive::External` during `encode()`. No per-frame callback parameter is needed; `encode()` signature simplifies.

### Seams

| Seam | Connects | Expects | Provides |
|------|----------|---------|----------|
| Parser protocol | `harbor-terminal` → `harbor-parser` | `VtHandler` trait impl | Parsed VT actions via `Parser::advance()` |
| PTY input | `harbor-terminal` → OS PTY | `impl Read + Send + 'static` | Byte stream consumed by internal reader thread |
| PTY output | `harbor-terminal` → OS PTY | `impl Write + Send + 'static` | Keystroke bytes written synchronously |
| CustomPaint registration | `CustomPaint` → `Runtime` | `BuildCx::register_external_draw(id, fn)` | Handler stored in Runtime's `HashMap<id, fn>` |
| External draw callback | `Runtime::encode` → `Terminal` | `ExternalDrawFn` signature: `(id, rect, &mut RenderPass)` | Terminal renders into the widget's allocated rect |
| Widget input | `Runtime` → `CustomPaint` → `Terminal` | `UiEvent` via `drain_external_input` | Terminal processes keyboard/mouse events |
| GPU injection | `CustomPaint` → `Terminal` | `&GpuContext` at render time | Terminal accesses atlas, pipelines, device/queue transiently |
| App widget setup | App → `harbor-widget` | `Runtime::set_root(CustomPaint::new(terminal.draw_id()))` | Terminal is a widget in the tree |

## End-to-End Tests

### E2E: Terminal renders via widget tree

- **Given:** App creates Terminal with PTY handles, wraps in `CustomPaint::new(draw_id)`, sets as runtime root
- **When:** PTY outputs `b"\x1b[31mHello\x1b[0m"`
- **Then:** Next `Runtime::encode()` call invokes Terminal's draw handler, red text "Hello" appears in the render pass

### E2E: Keystroke reaches PTY

- **Given:** Terminal widget has focus in widget tree
- **When:** User presses key `A`
- **Then:** Widget Runtime routes keyboard event → CustomPaint queues external input → App drains → Terminal writes `b"a"` to PTY output handle

### E2E: Terminal resize propagates

- **Given:** Terminal running at 80×24
- **When:** Window resizes to new dimensions → widget tree re-lays out → CustomPaint rect changes → Terminal.resize() called with new grid size
- **Then:** Terminal reallocates screen grid; next PTY output renders at new dimensions; scrollback preserved

### E2E: Multiple CustomPaint widgets coexist

- **Given:** Two independent Terminal instances with different draw IDs
- **When:** Both are added to widget tree (e.g., split panes)
- **Then:** Each renders into its own rect via its own registered handler; events route to the correct one based on hit testing

## Decisions

### CustomPaint registers handler during build, not app at encode time

- **Choice:** `BuildCx::register_external_draw(id, handler)` during widget build. Runtime stores a map.
- **Reason:** Eliminates app-layer glue (no per-frame callback parameter). Supports multiple CustomPaint widgets. Aligns with ADR-0005 (provider-by-ID) but shifts registration from app to widget build phase.
- **ADR reference:** [0005-custom-paint-provider-by-id](../adr/0005-custom-paint-provider-by-id.md), [0011-terminal-custompaint-gpu-injection](../adr/0011-terminal-custompaint-gpu-injection.md)

### Terminal does not own GpuContext

- **Choice:** `GpuContext` injected by CustomPaint at render time, not stored in Terminal.
- **Reason:** Terminal must not bind to a specific GPU device lifecycle. It uses pipelines owned internally but obtains device/queue/surface info transiently.
- **ADR reference:** [0011-terminal-custompaint-gpu-injection](../adr/0011-terminal-custompaint-gpu-injection.md)

### Parser exposes exactly three items

- **Choice:** Public API = `VtHandler` trait, `Params` type, `Parser` struct. All internal state private.
- **Reason:** Minimizes coupling surface while preserving zero-cost generic dispatch. Alternatives (event enum with allocation, closure-based with virtual dispatch) rejected for performance on the hot path.
- **ADR reference:** [0010-parser-minimal-public-api](../adr/0010-parser-minimal-public-api.md)

### Synchronous PTY I/O with std traits

- **Choice:** `impl Read + Write + Send + 'static`. No tokio, no bytes crate.
- **Reason:** Keep dependency footprint minimal. Internal thread handles blocking reads. Write latency for keystrokes is acceptable at synchronous speeds.
- **ADR reference:** [0013-synchronous-pty-io](../adr/0013-synchronous-pty-io.md)

### Delete harbor-ui; paste confirmation becomes widgets

- **Choice:** Dialog rendering moves to `harbor-widget` widgets or `harbor-terminal` internal modal. `harbor-ui` crate deleted.
- **Reason:** Spec requires widget tree handles all drawing. Dedicated UI crate with separate `ModalContent` trait adds unnecessary fragmentation.
- **ADR reference:** [0012-consolidate-render-ui-into-terminal](../adr/0012-consolidate-render-ui-into-terminal.md)

### Component trait from harbor-render dissolves

- **Choice:** The `Component` trait (`prepare` + `draw` + `resize`) in `harbor-render` goes away. Terminal rendering components become internal implementation details, not a public trait contract.
- **Reason:** Terminal is the only consumer. No other crate implements `Component`. The trait was an abstraction over a single implementation.
- **ADR reference:** [0012-consolidate-render-ui-into-terminal](../adr/0012-consolidate-render-ui-into-terminal.md)

### ADR-0006 Superseded

- **Choice:** ADR-0006 (CustomPaint Input Provider — App-owned input handling) is superseded. Terminal now handles its own input internally; the app only drains `external_input` and passes to Terminal without understanding event semantics.
- **Reason:** The "App continues to own terminal input handling" clause is no longer true. Input is fully encapsulated.
- **ADR reference:** [0006-custom-paint-input-provider](../adr/0006-custom-paint-input-provider.md) — status should be Superseded, pointing to this spec.

## Test Plan

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

## Future Evolution

- When widget framework supports native sub-windows, paste confirmation could become a widget-managed overlay instead of a winit window.
- When GPU resource lifetime management grows complex, Terminal's pipeline ownership could be extracted into a `TerminalGpu` sub-struct with explicit init/shutdown.
- If additional external draw providers emerge (image preview, web view), the `HashMap<id, fn>` pattern naturally extends — no spec changes needed.
- If PTY write latency becomes measurable under heavy paste, consider a buffered write path with `BufWriter`.

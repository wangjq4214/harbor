# Terminal Engine and wgpu Renderer Boundary

**Spec ID:** 0016
**Status:** Accepted (owner-confirmed)
**Date:** 2026-09-27

**Stable execution aliases:** 0014/T0001 = core/update boundary; T0002 = PTY ownership; T0003 = renderer consumption; T0004 = application/widget migration; T0005 = final dependency/Windows/performance acceptance below. Owner acceptance does not assert previously unrun checks ran. Baseline/final dependency graphs and one-/multi-session before/after measurements must use reproducible commands and identify environment/revision/dirty scope, without inventing a performance threshold. The delivered ownership/teardown map must match code; core-only and rendered targets must both build.

## Sources and scope

- [Issue #176](https://github.com/wangjq4214/harbor/issues/176) defines the requested outcome, exclusions, acceptance, verification, and sequencing; #169 and #170 are closed prerequisites.
- [ADR-0045](../adr/0045-gpu-independent-terminal-core-boundary.md) records the approved feature-isolation-first approach and ownership direction. [ADR-0011](../adr/0011-terminal-custompaint-gpu-injection.md) preserves host-provided GPU access; [ADR-0021](../adr/0021-external-draw-scheduling-and-standalone-terminal-host.md) preserves host scheduling; [ADR-0043](../adr/0043-acknowledged-pty-resize-barrier.md) describes the PTY resize ordering obligation.
- [Spec 0002](./0002-terminal-render-consolidation.md) describes the previously implemented integration of rendering into `harbor-terminal`; this specification changes that boundary for #176 without rewriting the historical contract. [Spec 0006](./0006-terminal-frame-scheduling-and-standalone-host.md) supplies frame-scheduling context.
- **Retired Spec 0005 — widget-boundary migration:** terminal-owned `RenderTarget`/`TerminalEvent` and application bridge responsibilities survive in the [embedding contract](../../docs/terminal/embedding.md) and [ownership map](../../docs/developer/terminal-session.md). The bridge maps draw geometry and routed input; Host input policy and external-draw ID matching guard input delivery, not rendering. It does not acquire PTY, native-resource, lifecycle or error policy. Preserve dependency proof, mapped-input/gate/ID regression coverage and no extra recurring redraw or full-scene rebuild; the existing 80×24 sub-2 ms frame target remains in Specs [0002](./0002-terminal-render-consolidation.md) and [0009](./0009-widget-decoration-and-terminal-chrome.md). The earlier phase retained winit, in conflict with ADR 0012's no-winit rule; that historical discrepancy is not erased or retroactively declared compliant by this later core-extraction contract.

## Requirement

The terminal's state, parser, input, selection, damage and timing must build and test without wgpu, arboard, winit or `harbor-widget`. The existing concrete wgpu renderer must consume an explicit engine-to-renderer update boundary without mutable access to parser or screen state. Existing `harbor-terminal` facade callers and application behavior remain compatible throughout migration, including PTY and GPU failure/teardown behavior, resize, DPI, palette, uploads, selection, cursor, scrollbar, preedit, wake and frame demand. Record ownership, dependency, performance and Windows runtime evidence before claiming completion.

## Solution boundary

First isolate the core within `harbor-terminal` behind a build/dependency configuration that actually compiles and tests without the excluded dependencies. A separate core crate is permitted, not required, if feature isolation cannot demonstrate the dependency direction reliably. Keep the wgpu renderer concrete; do not introduce a universal `dyn TerminalRenderer` or GAT-based pass abstraction without a second backend.

The logical engine owns screen and parser state, input and selection behavior, transient preedit and timing/frame-demand state; a session adapter owns PTY endpoints, reader, resize coordination and teardown. The renderer owns projection, upload decisions and GPU resources. For each session, one concrete wgpu renderer receives a coherent update derived from `TerminalSnapshot`, `UpdateDamage`, selection, preedit, viewport, appearance and frame-demand concepts, without mutable parser/screen access or a new multi-renderer subscription contract. Taking a snapshot is not consumption: pending visual changes must remain renderable when a tab is hidden, drawing is skipped or a draw fails. An update may be acknowledged only after renderer state is sufficient to draw the current content; failure or uncertain projection must permit replay or full reconstruction. Preserve existing full/incremental behavior, including appropriate invalidation on resize, DPI/viewport and palette changes; the implementation chooses affected layers and concrete acknowledgement mechanics without changing observable behavior. Preserve the existing retained/live draw distinction and the host's window, device, surface, submission and presentation ownership.

Keep a compatibility facade while `harbor-app` and terminal widget integration migrate, but do not require it to remain in the final architecture; remove it if all callers have migrated and behavior is verified. The ownership map must identify engine, session adapter, renderer, host and resource teardown both during and after migration. GPU initialization failures must not leak or prematurely consume PTY endpoints; session close must preserve reader reaping and the existing resize barrier/PTY ordering. Do not change visible behavior as part of this boundary change.

### Necessary seams

The engine/session supplies coherent read-only updates to one concrete renderer; the session owns ordered PTY resize and shutdown, and the host supplies frame-scoped GPU access, input, retained/live scheduling and window/surface lifecycle. These seams must retain the solution boundary above.

## End-to-end verification cases

- Build/test core-only and rendered targets and inspect excluded dependency paths.
- Exercise full/incremental output and every presentation input named above; a snapshot read or skipped/failed draw cannot consume pending changes.
- Exercise active-output resize/DPI, scrollback/selection, failure/retry, hidden-tab recovery, creation/renderer-init failure/tab switch/close, and blink/wake/live-retained scheduling without changing ownership or visuals.

## Test and evidence plan

- Historical T0001 core-boundary dependency graphs and CPU-side performance proxies describe intermediate revisions, not integrated Windows/GPU/ConPTY or post-migration performance evidence. Owner acceptance does not supply missing executions.
- The [session ownership and teardown map](../../docs/developer/terminal-session.md) preserves construction, endpoint, resize, failure/retry and close responsibilities. Historical T0002–T0005 checkbox or implementation states do not establish lifecycle or integrated execution evidence.
- Cover full/incremental upload, resize, DPI, palette, selection, cursor, scrollbar, preedit, wake and frame demand with focused tests; validate caller behavior and failure/teardown with integration tests during and after migration, without requiring the facade to remain in the final API.
- Record before/after dependency graphs and one-/multi-session performance results; keep conclusions in committed summaries and raw artifacts locally or in retrievable CI.
- Collect Windows runtime evidence for create, resize, tab switch/hide, close, renderer-init failure where reproducible, and sustained output. Record revision, dirty-tree scope, OS/ConPTY/GPU versions, expected and observed outcomes, artifacts and known exclusions; label each check PASS / FAIL / NOT RUN / BLOCKED honestly.
- Applicable quality gates are format, clippy across all targets/features with warnings denied, workspace tests, documentation, and focused core-only builds/tests; record unrun gates honestly under validation policy.

## Out of scope

No new renderer or backend marketplace, software/SVG output, PTY transport redesign, parser thread, broad screen-storage rewrite, visual behavior change or claim that this task makes N12 transport-feasible. Do not add a generic renderer or render-pass abstraction without a demonstrated second backend. This is an enabling boundary for #158, #159, #163 and #166, not implementation of those features.

# Terminal Engine and wgpu Renderer Boundary

**Spec ID:** 0016
**Status:** In Progress
**Date:** 2026-09-27

## Sources and scope

- [Issue #176](https://github.com/wangjq4214/harbor/issues/176) defines the requested outcome, exclusions, acceptance, verification, and sequencing; #169 and #170 are closed prerequisites.
- [ADR-0045](../adr/0045-gpu-independent-terminal-core-boundary.md) records the approved feature-isolation-first approach and ownership direction. [ADR-0011](../adr/0011-terminal-custompaint-gpu-injection.md) preserves host-provided GPU access; [ADR-0021](../adr/0021-external-draw-scheduling-and-standalone-terminal-host.md) preserves host scheduling; [ADR-0043](../adr/0043-acknowledged-pty-resize-barrier.md) describes the PTY resize ordering obligation.
- [Spec 0002](./0002-terminal-render-consolidation.md) describes the previously implemented integration of rendering into `harbor-terminal`; this specification changes that boundary for #176 without rewriting the historical contract. [Specs 0005](./0005-terminal-widget-boundary-migration.md) and [0006](./0006-terminal-frame-scheduling-and-standalone-host.md) supply existing widget and frame-scheduling integration context.

## Requirement

The terminal's state, parser, input, selection, damage and timing must build and test without wgpu, arboard, winit or `harbor-widget`. The existing concrete wgpu renderer must consume an explicit engine-to-renderer update boundary without mutable access to parser or screen state. Existing `harbor-terminal` facade callers and application behavior remain compatible throughout migration, including PTY and GPU failure/teardown behavior, resize, DPI, palette, uploads, selection, cursor, scrollbar, preedit, wake and frame demand. Record ownership, dependency, performance and Windows runtime evidence before claiming completion.

## Solution boundary

First isolate the core within `harbor-terminal` behind a build/dependency configuration that actually compiles and tests without the excluded dependencies. A separate core crate is permitted, not required, if feature isolation cannot demonstrate the dependency direction reliably. Keep the wgpu renderer concrete; do not introduce a universal `dyn TerminalRenderer` or GAT-based pass abstraction without a second backend.

The logical engine owns screen and parser state, input and selection behavior, transient preedit and timing/frame-demand state; a session adapter owns PTY endpoints, reader, resize coordination and teardown. The renderer owns projection, upload decisions and GPU resources. For each session, one concrete wgpu renderer receives a coherent update derived from `TerminalSnapshot`, `UpdateDamage`, selection, preedit, viewport, appearance and frame-demand concepts, without mutable parser/screen access or a new multi-renderer subscription contract. Taking a snapshot is not consumption: pending visual changes must remain renderable when a tab is hidden, drawing is skipped or a draw fails. An update may be acknowledged only after renderer state is sufficient to draw the current content; failure or uncertain projection must permit replay or full reconstruction. Preserve existing full/incremental behavior, including appropriate invalidation on resize, DPI/viewport and palette changes; the implementation chooses affected layers and concrete acknowledgement mechanics without changing observable behavior. Preserve the existing retained/live draw distinction and the host's window, device, surface, submission and presentation ownership.

Keep a compatibility facade while `harbor-app` and terminal widget integration migrate, but do not require it to remain in the final architecture; remove it if all callers have migrated and behavior is verified. The ownership map must identify engine, session adapter, renderer, host and resource teardown both during and after migration. GPU initialization failures must not leak or prematurely consume PTY endpoints; session close must preserve reader reaping and the existing resize barrier/PTY ordering. Do not change visible behavior as part of this boundary change.

### Necessary seams

| Seam | Connects | Expects | Provides |
| --- | --- | --- | --- |
| Terminal update | Logical engine/session → wgpu renderer | Coherent snapshot/damage and presentation inputs; snapshot reads do not consume damage; replay after skipped/failed draws | Full or incremental GPU preparation without mutable parser/screen access; acknowledge only safely applied updates |
| Session lifecycle | Session adapter ↔ `harbor-pty` and logical engine | Owned endpoints, acknowledged resize ordering and safe shutdown | Ordered parsing/geometry and resource release across success, failure and close |
| Widget and host integration | `harbor-app` / `TerminalWidgetBridge` ↔ terminal session and renderer (via facade during migration) | Existing input, render target, retained/live draw, wake and scheduling behavior | Existing widget paint integration without moving window/GPU lifecycle into core |
| GPU access | Host → wgpu renderer | Frame-scoped device, queue and render-pass access | Concrete terminal rendering without core GPU dependency |

## End-to-end verification cases

- **Core-only build:** Given the core-only target, when compiled and tested and its dependency graph inspected, then it has no compile-time path to wgpu, arboard, winit or `harbor-widget` while parser, input, selection, damage and timing tests run.
- **Output and incremental rendering:** Given a running terminal and PTY output, when dirty ranges are presented, then the renderer uses the applicable incremental/full update and draws the same text, cursor, selection, scrollbar and preedit as before; palette and viewport changes invalidate the affected rendering correctly.
- **Hidden or failed drawing:** Given output while a tab is hidden or a draw cannot complete, when that tab becomes drawable again, then the latest complete state is visible without losing changes, whether by replaying retained damage or rebuilding the necessary projection. A read-only snapshot must not clear pending damage.
- **Resize and DPI:** Given active output, scrollback and selection, when the host changes size or scale, then the existing PTY/model resize ordering and visual projection remain consistent, including failure and retry paths.
- **Session lifecycle:** Given a terminal under construction or closing, when GPU initialization fails or a tab is hidden, switched, or closed, then PTY endpoints and GPU resources obey their designated owners' teardown rules without leaking or dropping a live session early.
- **Scheduling and retained paint:** Given active and inactive terminals, when output, input, blink deadlines or retained draws occur, then wake/frame-demand and live/retained behavior remain compatible without the core owning a host surface.

## Decisions and open contract detail

- **Approved direction:** Feature isolation first, optional crate split only when needed to prove the required core-only dependency boundary; concrete wgpu renderer and a migration-only compatibility facade that may be removed once all callers have moved. Source: [ADR-0045](../adr/0045-gpu-independent-terminal-core-boundary.md) and #176.
- **Preserved boundaries:** Host-injected GPU access and host-owned scheduling; acknowledged PTY resize ordering remains a constraint. Sources: ADR-0011, ADR-0021 and ADR-0043.
- **Confirmed update and ownership policy:** One current renderer per session; snapshot reads do not consume visual changes, and hidden/failed draws must be recoverable. The logical engine owns state and timing; the session adapter owns PTY connection, resize coordination and close. Source: user-confirmed recommendations in [ADR-0045](../adr/0045-gpu-independent-terminal-core-boundary.md).
- **Implementation freedom:** Concrete update representation, safe acknowledgement mechanics and affected upload layers may be selected during implementation as long as the preceding behavior, existing visual output and dependency boundary are preserved. Any proposed observable or architectural change returns to discussion before revising the spec.

## Test and evidence plan

- Cover full/incremental upload, resize, DPI, palette, selection, cursor, scrollbar, preedit, wake and frame demand with focused tests; validate caller behavior and failure/teardown with integration tests during and after migration, without requiring the facade to remain in the final API.
- Record before/after dependency graphs and one-/multi-session performance results; store evidence under `docs/verification/` or link retrievable CI artifacts.
- Collect Windows runtime evidence for create, resize, tab switch/hide, close, renderer-init failure where reproducible, and sustained output. Record revision, dirty-tree scope, OS/ConPTY/GPU versions, expected and observed outcomes, artifacts and known exclusions; label each check PASS / FAIL / NOT RUN / BLOCKED honestly.
- Run applicable gates: `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test --workspace`, `python scripts/check_docs.py`, `python scripts/checklist_summary.py`. Record any gate not run and why, alongside focused core-only build/test commands. Follow [validation policy](../../docs/validation.md).

## Out of scope

No new renderer or backend marketplace, software/SVG output, PTY transport redesign, parser thread, broad screen-storage rewrite, visual behavior change or claim that this task makes N12 transport-feasible. Do not add a generic renderer or render-pass abstraction without a demonstrated second backend. This is an enabling boundary for #158, #159, #163 and #166, not implementation of those features.

## Sequencing constraint

The contract/design follows the established #169 boundary; ownership migration follows the first reflow path in #170. Both issues are closed, but the migration must preserve their resulting behavior. This spec defines the shared requirement contract; executable decomposition and order belong to tickets if that stage is selected.

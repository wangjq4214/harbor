# Embedding the terminal core

[Terminal core](README.md) · [Concepts](concepts.md) · [Security](security.md)

Harbor exposes a Rust engine and a concrete renderer, not a universal renderer trait, C ABI or complete native host. A custom embedder supplies the responsibilities below. Importing the widget runtime is not required by the engine.

## Dependency choices

The default `renderer` feature adds optional `wgpu` and `pollster`; `render` and its concrete types are feature-gated. The engine also depends on `harbor-parser`, `harbor-pty`, `harbor-config` and `harbor-text`.

```toml
# Headless parser/state/content consumer:
harbor-terminal = { path = "../harbor/crates/harbor-terminal", default-features = false }

# Alternative for a host using the concrete renderer (choose ONE declaration):
# harbor-terminal = { path = "../harbor/crates/harbor-terminal" }
# Or explicitly: default-features = false, features = ["renderer"]
```

A rendered host must arrange compatible wgpu device/queue/target-format access and font/metric setup. In this workspace these are supplied by `wgpu`, `harbor-text` and the native host. To spawn the supplied live backend, the host also uses `harbor_pty::PtyEndpoints` and `ShellCommand`; adding its own direct dependencies is the host project's responsibility.

Pipeline layers are private. Use `presentation_stats()` for diagnostics and pipeline-level viewport/palette/raster/font operations for resource changes; standalone layer types remain supported. `ScreenReader` was removed: use immutable `Screen::terminal_snapshot` and `Screen::selected_text` directly. `appearance_rgba` deliberately reports the construction-time tint; `clear_rgba` and update appearance expose current session colors.

Cargo features are additive. Check your dependency graph if another dependency may enable the renderer:

```sh
cargo tree -p harbor-terminal --no-default-features -e normal --prefix none
```

With renderer features disabled throughout the dependency graph, the terminal engine has no wgpu, winit, arboard or harbor-widget dependency.

## Select an ownership model

| Model | Entry point | What you must supply |
| --- | --- | --- |
| Parse/state only | `Terminal::new_headless(rows, cols)` | Ordered output bytes; event handling/discard policy; limits on your own work and retained copies. No PTY writes are available. |
| Supplied live backend | Headless engine, then `start_session_from_endpoints(endpoints, wake)` | `PtyEndpoints::spawn_shell` setup, event-loop wake route, host lifecycle/security policy; Windows bundled ConPTY distribution. |
| Test/custom stream adapter | `new_headless_with_io(rows, cols, reader, writer, wake)` | `Read + Send + 'static`, `Write + Send + 'static`, orderly EOF/unblocking and external transport lifecycle. This is not a general native PTY control API. |
| Custom projection | `snapshot` / `read_update` | One coherent incremental renderer baseline, or an independently managed full-copy projection; geometry/input mapping and presentation. |

The custom stream constructor starts a reader and panics if reader-thread creation fails. It attaches **no** `PtyControl`: it cannot resize or cancel an arbitrary external transport on your behalf. Its drop path does not provide the ConPTY reaper's cancellation/join guarantee; arrange reader completion yourself. Prefer the fallible `start_session_from_endpoints` for the supplied live session. Do not detach raw reader/writer/control parts and abandon their shutdown owner.

Keep parsing, screen mutation, input and update acknowledgement serialized on the logical owner thread. The native reader only queues bytes and calls `wake`; it must not call back into the engine, render or touch UI objects. `wake` returns whether notification succeeded: return true only when the host will service it; false stops the pump. Notifications are coalesced, so one wake is not one chunk and the callback must not block waiting for the logical thread.

## Practical host sequence

This sequence is pseudocode. Backticked terminal method names are public APIs; window, surface and event-loop operations belong to your host.

```text
CONSTRUCT
  Choose bounded grid, appearance and clipboard policy.
  Create Terminal::new_headless_with_appearance(rows, cols, appearance).
  Set clipboard delivery BEFORE any bytes are parsed.
  If using the supplied backend, create an intact PtyEndpoints bundle.
  If rendering:
    Initialize host device/queue, target format, fonts and TextMetrics.
    Construct TerminalRenderPipeline::new(gpu, surface_size, fonts, metrics,
                                          &terminal.read_update(now)).
    Do not acknowledge merely because the constructor succeeded.
  Attach the intact bundle with terminal.start_session_from_endpoints(endpoints, wake).
  Publish the session/renderer into the host only after fallible construction succeeds.

WAKE / ABOUT TO SLEEP
  Route wake to the originating session, not whichever tab is now active.
  Call terminal.frame_demand(now) (this drains queued output).
  Drain terminal.drain_output_events(); apply host policy or discard each request.
  Schedule demand.redraw_now and the earliest demand.deadline.
  Track demand.ordinary_present_eligible for synchronized output.

INPUT
  Resolve host shortcuts/paste authorization before terminal delivery.
  Map input into TerminalEvent; call handle_event_with_outcome.
  Apply returned redraw, capture_pointer, release_pointer and hyperlink_activation policy.
  Drain metadata/effect events produced by any newly ingested output too.

LIVE FRAME
  Acquire a usable host render target; defer zero-sized/occluded surfaces safely.
  Synchronize renderer raster scale/resources before using new metrics.
  Call terminal.prepare_render_frame(target, renderer.metrics(), now).
  Read terminal.read_update(now); keep this exact value for acknowledgement.
  Require renderer.sync_viewport(viewport, grid_changed) to succeed.
  Require renderer.prepare(gpu, &update, terminal.blink_visible_at(now)) to succeed.
  Encode renderer.draw(&mut pass) into the host pass.
  Acknowledge this exact update only when its projection has been safely consumed.
  Host submits/presents, handles surface/device errors and schedules retries.

CLOSE
  Revoke queued host effects, input routes, content jobs and wake routes for this session.
  Release mounted callbacks/bridge clones, renderer and terminal/session ownership.
  Let the supplied session transfer reader/control to its platform shutdown reaper.
  Host releases its own surface/device resources according to their lifetimes.
```

Construct fallible resources before publishing a session. Keep an unstarted `PtyEndpoints` bundle intact so setup failure can shut it down safely; do not publish a half-initialized session.

### What a host must not omit

- **Output independent of visibility.** Hidden sessions remain alive. A host can defer painting, but must deliberately choose when to drain output/events; a bounded queue eventually backpressures a producer. Never clear damage just because a tab is hidden. Avoid unbounded host wake/event queues.
- **Scheduling.** Query demand before sleeping, honor deadlines for blink/drag auto-scroll, and wake on output, input and geometry changes. `read_update` is read-only and does not drain PTY output. `frame_demand` does; keep that distinction explicit.
- **Synchronized output recovery.** `?2026` suppresses ordinary presentation, not parsing. The core owns a nesting count, not a wall-clock watchdog. Supply a bounded recovery/forced-live-present policy without lying to DECRQM by clearing the mode.
- **Coordinate mapping.** Supply `RenderTarget` allocation origin/size, surface size and scale consistently; feed host pointer coordinates through the terminal's current viewport. Do not pre-encode mouse positions using stale modes: queued output can switch cell/pixel encoding. Distinguish physical pixel coordinates from wheel delta units.
- **Input precedence.** Route keyboard, pointer, real focus gain/loss, paste, preedit and IME commit coherently. Terminal reporting modes outrank local pointer selection where applicable. Respect capture/release outcomes and cancel interaction on focus loss/unmount.
- **IME.** Preedit is transient overlay state, not screen text or PTY input. Route committed text as keyboard IME input once; do not also inject it as synthetic key events or advance the cursor ahead of child echo. Position candidates with `ime_candidate_position_with_metrics` and clear composition on cancellation/focus loss.
- **Effects.** Titles, OSC 7 and OSC 133 are metadata. Clipboard requests and hyperlink activation require host authorization. No emitted request authorizes OS access by itself; see [security](security.md).
- **Diagnostics.** Surface input/resize/startup errors rather than silently treating them as success. Reader/reply-write errors are currently logged, not delivered as a structured per-error host event; observe closure and expose an appropriate host status without leaking payloads.

## Read and acknowledgement contract

`TerminalUpdate` is an engine-issued, immutable acknowledgement token with read-only access to its complete visible `TerminalProjection` and incremental damage. Configuration preparation returns only a `TerminalProjection`, which renderers can consume but engines cannot acknowledge. Its `Ranges` apply relative to the most recently acknowledged state of that engine; it does not maintain per-subscriber cursors. Use one renderer per baseline. Multiple read-only content consumers are fine; independent incremental renderers sharing acknowledgements are not supported.

### Projection ownership and copying

`TerminalUpdate::snapshot` is an `Arc<TerminalSnapshot>`. The snapshot's `cells` is a `SnapshotCells` collection with flat `usize` indexing, `get`, `len`, `is_empty`, `iter` and explicit synthetic mutation support. It is not a contiguous `Vec<Cell>`: use `cells.iter().cloned().collect::<Vec<_>>()` only when a consumer actually requires a flat owned copy. For a logical row, prefer `snapshot.cell(row, col)` or iterate the desired flat indices; arbitrary slice indexing and Vec growth/removal are not provided.

Ordinary engine reads reconcile visible row versions and share unchanged rows, including when older updates remain alive. A changed row copies its cells/suffixes; there is no per-cell sharing claim. The engine caches only its latest visible row directory, not all scrollback. Reads still process row metadata/damage and allocate the small snapshot/update metadata; this is not an O(1) entire-frame guarantee. Initial/resize/reconstruction work may be full.

Keep the exact engine-issued update for acknowledgement. Its projection is immutable (no `DerefMut`). To render synthetic data, explicitly clone `update.projection()` and mutate that detached `TerminalProjection`; it has no acknowledgement capability. Cloning a `TerminalSnapshot` shares its cell rows; synthetic cell mutation uses copy-on-write for the touched row (plus the row directory). Building `SnapshotCells` from a flat Vec creates one storage chunk, so mutating that chunk can copy all of it.

Validity uses checked mutation revisions, distinct from semantic content-reader revisions and the projection acknowledgement/invalidation epoch. It includes buffer/cursor/colors/synchronized-output, selection/auto-scroll, blink and preedit/backdrop state. Relevant A → B → A changes reject old acknowledgement. Mutable owner access may conservatively invalidate; an empty PTY scheduling poll does not itself mutate the selection owner. No grid equality scan is used to revive an old token.

Here is a complete, compilable-API helper for a **custom projection**, with projection work supplied by the caller. It does not create a renderer or present a frame:

```rust,no_run
use harbor_terminal::{Terminal, TerminalUpdate};
use std::time::Instant;

fn project_once(
    terminal: &mut Terminal,
    prepare_and_commit: impl FnOnce(&TerminalUpdate) -> bool,
) -> bool {
    let update = terminal.read_update(Instant::now());
    if !prepare_and_commit(&update) {
        // Conservatively rebuild after a possibly partial/uncertain projection.
        terminal.invalidate_update();
        return false;
    }
    terminal.acknowledge_update(&update)
}
```

The callback's true result must mean the caller retained everything needed to replay this state. It is not an acknowledgement of merely receiving a snapshot. A false acknowledgement means state/epoch no longer matches: keep work pending and obtain a fresh update. Do not loop on the stale object. There is no public manual damage-clearing bypass.

The reference bridge acknowledges after successful preparation and draw encoding; that is **not proof the OS has displayed the frame**. The host owns submit/present failures. If retained GPU data remains trustworthy, replay it; if resources or the projection become uncertain, call `invalidate_update`, rebuild and do not acknowledge a failed preparation. Recreating the pipeline from a range update without forcing a full baseline is not a safe recovery strategy.

Retained draws may replay committed buffers while geometry and input mapping remain valid. Changed origin/allocation/surface/scale, font metrics, resource replacement or pending full reconstruction require appropriate live preparation. The bridge uses `retained_geometry_changed`, renderer viewport checks and `sync_raster_scale`; a DPI resource change invalidates the engine projection. Empty cell-damage ranges do not mean cursor/selection/preedit/appearance can be ignored.

## Resize transaction and failure behavior

Use `request_resize(TerminalSize { rows, cols })` to publish intent without committing geometry. It returns `Unchanged`, `ConfigurationOwned`, or `Pending(ResizeStatus)`. Poll `resize_status()` to distinguish `WaitingForReader`, `ReadyToCommit`, and `Failed`; failed acquisition is not readiness, but `needs_advance()` includes failure cleanup. Schedule `resize_deadline` and drain output even for hidden sessions. Call `advance_resize()` in bounded per-host-turn admission slots; only `Committed(size)` means geometry was published, and failures return `Err` after reader release. For multi-tab hosts, enable `use_host_resize_scheduling` so automatic drains/draws cannot bypass admission. Dimensions normalize to at least one row and two columns; ConPTY's signed-16-bit limits still apply. Validate a reasonable maximum before allocating.

The supplied live session requests an epoch-qualified reader barrier, interrupts a blocking read, and parses all completed pre-barrier chunks against the old geometry. It prepares new screen/pointer state detached from the live model, resizes the PTY, then commits through ownership moves. The barrier guard resumes on success and error. Timeout (currently two seconds), interrupted-read/control error, preparation failure or PTY resize failure must not commit candidate geometry.

Failure can still leave newly ingested **old-geometry output** visible; "resize failed" is not "no output was processed." The barrier orders reader-observed chunks, not every byte still inside the OS pipe. Keep partial parser sequences across resize; do not strip application clears/redraws or reset the parser to manufacture a boundary. Primary reflow and alternate clipping retain their documented losses; resize is not a lossless archival operation.

The compatibility `try_resize_if_changed` wrapper returns true only on commit; `resize_if_changed` logs errors, and `resize` discards that boolean. `prepare_render_frame` retains this convenience path for standalone engines. Error-aware or multi-tab hosts should use the explicit request/progress API and track committed geometry, not merely requested allocation.

## Teardown, EOF and host failures

Dropping the supplied live terminal transfers its reader join handle and `PtyControl` to the platform shutdown protocol. On Windows the reaper cancels/observes reader completion before closing ConPTY; it does not join on the UI thread. Renderer destruction does not close the PTY. Conversely, closing/hiding a native surface does not close a session while other terminal/bridge references still own it.

Reader EOF/disconnect is distinct from host close: drain remaining queued bytes, observe `is_session_closed`, cancel pending clipboard authority, and decide whether to retain the exited screen. Disconnect releases synchronized-output suppression; it does not discard retained searchable content. A headless engine without a reader remains logically open. Do not infer a child exit code or reason from this boolean.

Release all callback clones and pending host jobs when truly closing. Dropping a detached `PtyControl` without its reader cannot safely reap the session; the defensive implementation can leak rather than close ConPTY under a live read. Keep endpoint ownership intact instead of relying on that fallback.

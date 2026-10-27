# Terminal session ownership

A terminal engine, its PTY session and its GPU projection have different responsibilities and lifetimes. Public construction and update contracts are in [embedding](../terminal/embedding.md); this guide explains the internal ownership boundary.

## Owners

| Owner | Resources and responsibility |
| --- | --- |
| `Terminal` and logical `TerminalIo` state | Screen, parser, input modes/encoding, pointer/selection, preedit, content revisions, update damage, blink and frame demand |
| `TerminalSession` in `io.rs` | Sole optional `TerminalPty`: reader/writer/control, bounded output queue, wake flag, resize barrier commands/epochs, disconnect observation and teardown |
| `TerminalRenderPipeline` | Retained projection, uploads, GPU resources and render viewport; no PTY or mutable parser ownership |
| `TerminalWidgetBridge` | Shared engine/renderer references and draw/input/IME/schedule callbacks; mounting is not session ownership |
| `TabManager` / native host | Tab lifetime and source-qualified dispatch / window, surface and presentation lifetime |

The engine has no renderer field. Core-only construction requires neither GPU resources nor OS handles, and the `renderer` feature can be disabled. A headless bridge has no GPU projection. Neither a rendered bridge nor the engine owns a host surface.

## Construction and failure

The product factory in [`tab_coordinator.rs`](../../src/tab_coordinator.rs) follows this order:

1. Spawn an unstarted `PtyEndpoints` bundle and create a headless logical engine with accepted appearance/policy.
2. Initialize the concrete renderer from a read-only engine update **before** transferring endpoints. GPU initialization failure drops the intact endpoint bundle.
3. Transfer endpoints into the session and start its reader with a tab-qualified wake callback. There is no second PTY owner in the bridge.
4. Retain engine and bridge in `TerminalTabResources`; mount only the active bridge.

If reader thread creation fails after transfer, the captured reader is dropped and control uses `shutdown_unstarted`. Dropping detached ConPTY control without that shutdown path is not equivalent cleanup.

## Reader, model and renderer flow

The blocking reader publishes bounded chunks; the UI thread drains and parses them. Wake coalescing remains armed until the queue is empty, with a second receive after clearing the flag to close the producer/consumer race. Protocol replies use the same session writer. Hidden tabs continue ingesting output even though their external provider is not mounted.

The bridge reads `TerminalUpdate`, prepares/synchronizes the renderer, draws, then acknowledges that exact update. Preparation failure leaves damage pending. Acknowledgement rejects intervening engine/projection changes using session-qualified mutation provenance and exact shared snapshot identity, not cell equality. Visible cell projections reuse row payloads according to row versions; a changed row is copied even if an older update remains retained, without detaching every row. Overflow-recognition bookkeeping explicitly invalidates its cached row without changing independent semantic-reader revisions or visual damage. Renderer loss/recreation requires `invalidate_update` and full reconstruction, not silently treating pending ranges as consumed. This damage/acknowledgement stream is for one retained renderer projection, not independent competing renderers.

Read-model consumers and literal search have their own revision/provenance contracts; they do not acknowledge render damage. Unmount clears transient preedit but does not itself destroy the retained tab session.

## Resize invariants

1. Derive grid geometry from the committed terminal allocation, DPI and font metrics. Reject non-drawable allocations; normalize accepted grids to at least one row and two columns.
2. Request an epoch-qualified reader barrier and interrupt a blocked read. Stream completed chunks through the parser against the **old** grid, including replies; do not collect a refilling queue into an unbounded staging vector.
3. Wait for the matching acknowledgement. Keep the reader paused during model preparation, PTY resize and coherent model commit.
4. Resume through the barrier guard on success and error. A timeout must not commit a resize; retries use a new epoch so stale acknowledgements cannot satisfy them.

The barrier orders **reader-observed chunks**, not bytes still unseen in the OS pipe; it is not a VT stream boundary and must preserve partial parser sequences. Primary/history resizing reflows within finite retention; alternate-screen resizing is rectangular. Windows live ConPTY viewport policy differs from headless history-pulling policy. See [terminal concepts](../terminal/concepts.md) for content/viewport distinctions.

## Hide, close and EOF

Switching/hiding retains tab resources. Closing removes the intended stable tab identity, cancels source-qualified work and drops the tab's engine/bridge references. Callback clones and other references can keep resources alive until their final drop; do not equate removal from the visible tree with immediate GPU or PTY destruction.

On final session drop, `TerminalPty` transfers reader/control cleanup to the platform shutdown/reaper path and drops its writer. The UI thread need not join a blocked reader. The renderer releases only its own GPU resources; surface teardown remains native-host responsibility.

Reader disconnect is observed once and releases synchronized-output suppression. EOF does not implicitly discard retained terminal content or release all remaining endpoint/control resources; those remain owned until close. Late wakes for removed tabs must not affect a replacement tab. Clipboard/hyperlink effects still require host policy; see [security](../terminal/security.md).

## Source map

- [`lib.rs`](../../crates/harbor-terminal/src/lib.rs): engine construction, frame demand and read/acknowledge/invalidate update contract.
- [`io.rs`](../../crates/harbor-terminal/src/io.rs): queue, reader pump, session, barriers and parser/writer coordination.
- [`harbor-pty/src/lib.rs`](../../crates/harbor-pty/src/lib.rs) and [`windows.rs`](../../crates/harbor-pty/src/windows.rs): endpoint transfer, interrupt, resize and shutdown.
- [`render/pipeline.rs`](../../crates/harbor-terminal/src/render/pipeline.rs): concrete projection lifetime.
- [`terminal_view.rs`](../../crates/harbor-app/src/terminal_view.rs) and [`tab_manager.rs`](../../crates/harbor-app/src/tab_manager.rs): callback sharing, mounting and tab lifetime.

See [architecture](architecture.md) and [widget runtime](widget-runtime.md) for the surrounding host flow.

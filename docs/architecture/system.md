# System Architecture

This is the current end-to-end map, not a proposal for a new runtime. It connects the [application handbook](../app/README.md), [widget guide](../widget/README.md) and [terminal guide](../terminal/README.md). Long-term comparisons and non-goals live in [Targets](../targets.md); exact behavior and acceptance remain in linked contracts.

## Layers and Dependency Direction

```text
harbor binary: OS event loop, startup, platform authorization, configuration transactions
  + harbor-app: tabs/workspace, commands, search, notifications, product appearance
  |    + harbor-widget: descriptions, state, identity, layout, input, retained scene
  |    |    + optional winit host: native windows, surfaces, scheduling, presentation
  |    + terminal bridge: CustomPaint draw/input/IME/schedule providers
  |         + harbor-terminal: logical engine, session adapter, optional renderer
  |              + harbor-parser: streaming VT framing and callbacks
  |              + harbor-pty: endpoint/control ownership (Windows ConPTY)
  |              + harbor-text: font selection, fallback, metrics, rasterization
  + harbor-config: TOML loading, validation, defaults and launch settings
```

This is an ownership/dependency map, not a claim that every dependency is always compiled. The terminal `renderer` feature is default-on but can be disabled. Widget native hosting is feature-gated. See the crate manifests for the exact build graph.

**Direction matters:** generic widgets do not interpret terminal sessions, search queries or application commands. The engine does not own an OS window, clipboard authorization or a widget runtime. `harbor-app` composes generic mechanisms into the product; the binary applies platform effects and coordinates native windows.

## Ownership and Lifetime

| Owner | Retained state/resources | Lifetime boundary |
| --- | --- | --- |
| Binary `Shell` / `ActiveSession` | Event-loop dispatch, main host, confirmation windows, clipboard/paste policy, accepted configuration and watcher coordination | Application/window lifecycle |
| `TabCoordinator` / `TabManager` | Ordered tabs, stable `TabId`s, session resources, active selection and current-session search | Closing removes the intended tab; switching/hiding retains its resources |
| `TerminalWidgetBridge` | Shared engine reference, draw ID, renderer and draw/input/IME/schedule callbacks | Mounting is distinct from engine ownership; old callback clones can retain resources until dropped |
| `Terminal` | Screen, parser/input state, pointer/selection, transient preedit, revisions, damage and frame demand | Logical engine/session instance; core-only construction requires neither GPU nor OS handles |
| Terminal session adapter | PTY reader/writer/control, bounded output queue, wake/barrier state and shutdown | Owns the sole PTY session; drop transfers blocking reader cleanup to the platform reaper |
| `TerminalRenderPipeline` | Concrete retained projection, uploads, atlas/render resources and viewport | Renderer recreation is not session recreation; it does not own a host surface |
| Widget `Runtime` | Fibers, subscriptions, focus/capture, committed layout and retained scene | One runtime per OS window |
| `WinitWindowHost` / `SharedGpu` | Per-window surface/adapter/presenter; shared device/queue respectively | Surface and input state stay per-window even when GPU resources are shared |

The current product has one terminal session per tab, with only the active terminal bridge mounted. This is **not** yet the future Window → Tab → Split tree → Pane → Session workspace. Stable engine session identity is also distinct from application `TabId` and transient viewport coordinates. See [terminal ownership](terminal-session-ownership.md) and N07/N17 in the [product plan](../next-stage-plan.md).

## Input to Terminal Bytes

```text
WindowEvent
  -> native adapter: logical coordinates, modifiers, focus/IME state
  -> runtime route: capture / target / bubble, focused control and shortcuts
  -> either widget-local editing / typed application intent
  -> or terminal bridge -> engine input/mode policy -> PTY writer
```

1. The binary routes the event to the correct native window and applies cross-window safety gates.
2. The widget adapter normalizes native input. The runtime owns focus, hit testing, pointer capture and propagation within that window.
3. An available application shortcut can consume the event and enqueue typed intent. `Consumed` blocks terminal delivery; `PassThrough` must not enqueue the same command. Copy-or-interrupt is context-sensitive.
4. A focused text field owns its editing, clipboard completion and IME. Accepted committed changes become application intent; preedit is not a search query or terminal input.
5. Unconsumed terminal input is interpreted using terminal modes and encoded to PTY bytes. Mouse tracking and local selection are distinct policies.
6. After routing/effects, the application drains its command queue and applies business mutations. A Store dispatcher is FIFO transport, not an automatic reducer, effect executor or wakeup mechanism.

Do not add a second command bus in a palette or menu. Do not send a consumed search/editor chord to the PTY. See [commands](../app/commands.md), [widget concepts](../widget/concepts.md) and [text input](../widget-input.md).

## PTY Output to a Frame

```text
blocking PTY reader
  -> bounded queued chunks + tab-qualified wake
  -> UI thread drains bytes -> parser -> screen / replies / output events
  -> app updates title, metadata, unread state and authorization requests
  -> host invalidates the mounted external draw
  -> runtime update/layout -> retained scene -> terminal draw -> submit/present
```

- The reader thread transports bytes; the logical parser/model and widget signals are not mutated by that thread. A wake is not itself a frame or proof that the tab remains active.
- `TabManager::process_output` resolves the stable source, drains output and classifies title/metadata/clipboard/unread changes. Hidden tabs continue ingesting output without mounting their draw provider.
- Terminal replies return through the session I/O path. Host-facing output events such as OSC 52 are requests with source identity, not permission to act on the active window indiscriminately.
- The engine publishes GPU-independent update data. The concrete renderer prepares that projection; acknowledgement follows successful consumption. Failed or lost projections require reconstruction rather than silently dropping damage.
- `WinitWindowHost` owns acquire/encode/submit/present. Widget and terminal drawing preserve paint order, allocation and clipping in the same host presentation lifecycle.

See [embedding](../terminal/embedding.md) for the read/acknowledge/replay and single-renderer limitations. A read-model/search consumer never acknowledges rendering damage.

## Search Work Is Not Paint Work

The application owns the query/source lifecycle and advances bounded search from host idle turns. One cooperative step reads a bounded page; completion is published only after current query/session/content validation. Navigation validates again before using a target.

The bridge receives neutral proven `ContentRange`s. Draw projects only the referenced visible current geometry; it does not scan all history, copy a full transcript or run the matcher. Appending output may preserve proven positives without certifying complete negative coverage. Resize/reflow, eviction and engine replacement invalidate incompatible provenance.

The search field, result labels and glass material do not own terminal content or matching policy. See [integration](../current-session-search.md), [read model](../terminal-read-model.md) and [literal consumer](../terminal-literal-search.md).

## Geometry, Resize and Configuration

Widget layout commits logical-pixel allocations atomically. The application observes the terminal allocation and derives physical terminal geometry with the current DPI and font metrics; raw window size is not interchangeable with the embedded content rectangle.

A session resize pauses the reader through an epoch-qualified barrier, parses already-observed bytes against the old grid, prepares the model change, resizes the PTY and commits coherent geometry. The reader resumes on success/error. Primary/history content reflows within finite retention; alternate content uses rectangular policy. The barrier does not order bytes still unseen in the OS pipe.

Configuration reload is a separate application transaction:

1. Read and strictly validate one complete candidate from the startup-selected path.
2. Prepare applicable resources and update session geometry through normal resize contracts.
3. Publish a coherent accepted configuration and future-session factories, or reject and retain the accepted generation.
4. Report compensation/recovery failures explicitly; do not call a failed rollback successful atomic recovery.

Live terminal settings, future-child launch settings and restart-required clipboard policy differ. Configuration watching only triggers this transaction; widget-library HMR is a separate debug feature. See [configuration](../app/configuration.md).

## Scheduling, Failure and Shutdown

| Situation | Owning response |
| --- | --- |
| Quiet runtime | Host waits unless dirty work or a deadline requests progress; no unconditional redraw loop |
| Search still pending | Application requests more host turns; bounded completion is not tied to hover or frame rate |
| Synchronized output | Engine frame demand defers ordinary presentation with bounded recovery; it does not authorize a stopped reader |
| Minimized / zero-size / occluded surface | Native host handles drawable/recovery policy without changing session ownership |
| Lost/outdated surface | Native host reconfigures; fatal GPU outcomes return to application policy |
| PTY EOF / child exit | Observe closed session state; retained content can still be read. EOF is not automatic deletion of all search content |
| Tab close | Cancel source-qualified work and release tab ownership; later events cannot target a reused identity |
| Final session drop | Session adapter reaps the blocked reader and releases ConPTY; renderer drops its own GPU resources |

The application owns cross-window paste and OSC 52 authorization, including foreground/source eligibility. Terminal metadata never grants local filesystem or command-execution authority. Local backdrop filtering samples Harbor's own prior scene, not arbitrary desktop pixels. See [security](../terminal/security.md).

## Source Map and Verification

| Area | Read |
| --- | --- |
| Startup/event dispatch and idle orchestration | [`src/shell.rs`](../../src/shell.rs), [`event.rs`](../../src/event.rs) |
| Factory, configuration/resize coordination | [`tab_coordinator.rs`](../../src/tab_coordinator.rs) |
| Commands, workspace and search | [`harbor-app`](../../crates/harbor-app/src/), especially `command.rs`, `workspace.rs`, `search.rs`, `tab_manager.rs` |
| Terminal/widget bridge | [`terminal_view.rs`](../../crates/harbor-app/src/terminal_view.rs) |
| Runtime and native hosting | [`harbor-widget`](../../crates/harbor-widget/src/), [runtime architecture](widget-runtime.md) |
| Engine and session transport | [`Terminal`](../../crates/harbor-terminal/src/lib.rs), [`io.rs`](../../crates/harbor-terminal/src/io.rs) |
| Concrete terminal renderer | [`render/`](../../crates/harbor-terminal/src/render/) |
| Platform authorization and configuration watching | [`clipboard.rs`](../../src/clipboard.rs), [`dialog.rs`](../../src/dialog.rs), [`configuration_watch.rs`](../../src/configuration_watch.rs) |

Focused contract tests and historical runtime evidence establish only their stated scope. This architecture description is not a new Windows compatibility, renderer-failure, multi-window, performance or release pass. Follow [Validation](../validation.md) for required evidence and [Current Status](../current-status.md) for exclusions.

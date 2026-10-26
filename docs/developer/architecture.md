# System architecture

This internal guide maps ownership and data flow. For usage, start with the [application](../app/README.md), [widget](../widget/README.md) and [terminal](../terminal/README.md) guides.

## Layers and dependency direction

```text
harbor binary        event loop, platform authorization, configuration transactions
  harbor-app         workspace/tabs, commands, search, notices, product appearance
    harbor-widget    view/state, identity, layout, input, retained GPU scene
      optional winit native windows, surfaces, scheduling, presentation
    terminal bridge  ExternalSurface draw/input/IME/schedule providers
      harbor-terminal  logical engine, PTY session adapter, optional renderer
        harbor-parser  streaming VT framing
        harbor-pty     endpoint/control ownership (Windows ConPTY)
        harbor-text    fonts, fallback, metrics, rasterization
  harbor-config      configuration loading, validation and defaults
```

This is an ownership map, not the exact Cargo dependency graph. Terminal rendering is a default-on feature; widget native hosting is opt-in. Generic widgets do not interpret terminal sessions or application commands. The logical engine owns neither an OS window nor clipboard authorization. Product composition belongs to `harbor-app`; the binary coordinates platform effects and native windows.

## Ownership boundaries

| Owner | Retained responsibility |
| --- | --- |
| Binary `Shell` | Event-loop dispatch, main/confirmation hosts, cross-window safety and exit policy |
| `TabCoordinator` / `TabManager` | Stable tab identities, session resources, active selection and current-session search |
| `TerminalWidgetBridge` | Shared engine reference, draw identity, renderer and external callbacks |
| `Terminal` / session adapter | Logical terminal state / sole PTY owner and reader lifecycle |
| `TerminalRenderPipeline` | Retained GPU projection, uploads and terminal viewport; no host surface |
| Widget `Runtime` | Fiber tree, focus/capture, committed layout and retained scene, per window |
| `WinitWindowHost` / `SharedGpu` | Per-window native/surface/presentation lifecycle / shared device and queue |

Each tab retains one terminal session; only the active bridge is mounted. Switching tabs is not session destruction. Engine session identity, application `TabId` and viewport coordinates are different identities. See [terminal session ownership](terminal-session.md) and [widget runtime](widget-runtime.md).

## Input flow

```text
native WindowEvent -> adapter normalization -> runtime capture/target/bubble
  -> widget-local editing or typed application intent
  -> otherwise terminal bridge -> terminal mode/encoding policy -> PTY writer
```

The binary selects the destination window and applies cross-window gates. The runtime owns focus, hit testing and capture within that window. A consumed application shortcut must not also reach the terminal. A focused text field owns local editing and IME; preedit is not committed search text or PTY input.

After routing, the application drains typed intents and applies business mutations. A Store dispatcher is FIFO transport, not an automatic reducer, effect executor or wakeup mechanism. Reuse the command path for menus and shortcuts rather than creating parallel buses. See [commands](../app/commands.md) and [text input](../widget-input.md).

## Output and presentation flow

```text
blocking PTY reader -> bounded chunks + source-qualified wake
  -> UI-thread drain -> VT parser -> screen, replies and output events
  -> application metadata/unread/authorization handling -> external invalidation
  -> runtime update/layout -> retained scene + terminal draw -> submit/present
```

The reader transports bytes; it does not mutate widget signals or parse the logical model. `TabManager::process_output` resolves the source tab, including hidden tabs. A queued wake neither proves the source still exists nor makes it the active tab. Terminal replies use the session writer; OSC 52 events are source-qualified authorization requests, not clipboard permission.

Reading a terminal render update does not clear damage. The bridge acknowledges only after successfully preparing the exact projection; uncertain retained state requires full reconstruction. Read-model/search consumers do not acknowledge rendering damage. Surface acquisition and presentation remain host-owned even when terminal and widget draws share paint order and clipping. See [embedding](../terminal/embedding.md).

## Search, geometry and configuration

- **Search:** the application advances bounded work on host turns, independently of paint. Completion/navigation validate query, session and content provenance. The bridge projects proven `ContentRange`s; drawing does not scan history. Surviving positives do not prove complete negative coverage.
- **Geometry:** terminal grids derive from committed widget allocation, DPI and font metrics, not raw window size. Resize drains reader-observed bytes under old geometry before barrier-protected model/PTY commit; see [resize invariants](terminal-session.md#resize-invariants).
- **Configuration:** validate and prepare a complete candidate before publication. Live settings, future-session launch settings and restart-required clipboard policy differ. Report compensation failure explicitly. Watching and widget HMR are separate; see [configuration](../app/configuration.md).

Quiet hosts wait for dirty work or deadlines. Synchronized output defers a terminal projection, not ingestion or neighboring widgets. Surface recovery is host-owned; EOF does not delete retained content. Terminal metadata grants no filesystem or command authority; see [security](../terminal/security.md).

Source entry points: [`shell.rs`](../../src/shell.rs), [`tab_coordinator.rs`](../../src/tab_coordinator.rs), [`harbor-app`](../../crates/harbor-app/src/), [`terminal_view.rs`](../../crates/harbor-app/src/terminal_view.rs). Follow [widget runtime](widget-runtime.md), [terminal session](terminal-session.md) and [profiling](profiling.md) for deeper ownership and measurement procedures.

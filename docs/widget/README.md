# WIDGET — compose a native interface

Harbor's **WIDGET** layer turns Rust component descriptions into a retained GPU interface. Use it for layout, controls, local editing state, focus and overlays; let the application own commands and policy, and let the native host own windows and presentation.

Harbor is a pre-1.0 workspace toolkit, not a complete desktop framework. Pin the repository revision when consuming it outside the workspace.

## Choose a path

| If you want to… | Read |
| --- | --- |
| Build your first tree, retain state, then submit application intent | [Learn WIDGET](learn.md) |
| Understand identity, hooks, signals, actions, effects and invalidation | [Mental models](concepts.md) |
| Find an existing control and its support boundary | [Widget catalog](catalog.md) |
| Declare sizes, insets and positioning; recover invalid configuration | [Explicit layout and fallible nodes](layout.md) |
| Connect a native host, editing controller or root overlay | [Composition recipes](recipes.md) |
| Test your components and troubleshoot integration | [Testing](testing.md) |

Start with **Learn**, then use the catalog to choose primitives rather than recreating input, focus or painting machinery. The recipes are compositions of current APIs, not proposals for new widgets.

## Where WIDGET fits

```text
Application: commands, published state, sessions, search policy
    ↓ component descriptions and queued intent ↑
WIDGET: reconcile → layout → input routing → retained scene
    ↓ runtime effects / frame work ↑ native events
Host: window, clipboard, IME, surface, scheduling, presentation
```

- A `Component` describes a `View`; the retained fiber owns identity and hook slots.
- A `Signal` holds UI-thread state. Writes invalidate subscribers; builds pull current values.
- A `Store` separates published state from one-shot actions. Dispatch queues intent; it does **not** reduce state or wake a sleeping host.
- `WinitWindowHost` supplies the ordinary native integration. A bare `Runtime` is useful for deterministic tests and custom hosts, but does not create a window.
- `CustomPaint` embeds an externally owned renderer. A terminal is one consumer, not a dependency of the widget library.

## Install and orient

Use the [crate README](../../crates/harbor-widget/README.md) for path dependencies and feature flags. Core has no default features; native hosting uses `winit`.

Related guides: [APP](../app/README.md), [TERMINAL](../terminal/README.md), [flex layout](../flex-layout.md), and [single-line input](../widget-input.md).

# WIDGET — compose a native interface

Harbor's **WIDGET** layer turns Rust component descriptions into a retained GPU interface. Use it for layout, controls, local editing state, focus and overlays; let the application own commands and policy, and let the native host own windows and presentation.

This guide borrows the learning progression of React Learn and the task-oriented browsing of Flutter's catalog, **not their APIs or compatibility promises**. Harbor is a pre-1.0 workspace toolkit, not a complete desktop framework. Pin the repository revision when consuming it outside the workspace.

## Choose a path

| If you want to… | Read |
| --- | --- |
| Build your first tree, retain state, then submit application intent | [Learn WIDGET](learn.md) |
| Understand identity, hooks, signals, actions, effects and invalidation | [Mental models](concepts.md) |
| Find an existing control and its support boundary | [Widget catalog](catalog.md) |
| Connect a native host, editing controller or root overlay | [Composition recipes](recipes.md) |
| Decide what a passing test actually proves | [Testing and acceptance](testing.md) |

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

Use the [crate README](../../crates/harbor-widget/README.md) for path dependencies and feature flags. Core has no default features; native hosting uses `winit`. No documentation website or extra runtime package is required for this guide.

Related product lines: [targets and support](../targets.md), [system architecture](../architecture/system.md), [APP](../app/README.md), and [TERMINAL](../terminal/README.md). Detailed existing references: [widget runtime architecture](../architecture/widget-runtime.md), [flex layout](../flex-layout.md), and [single-line input](../widget-input.md).

## Delivery is not acceptance

`TextField`, `EditableText`, `Portal`, local backdrop blur, linear-gradient fills/strokes and optional edge refraction have implementations today. They must not be treated as missing foundations. Conversely, their existence does not establish native IME, visual/DPI, GPU-backend or hot-reload acceptance.

The [testing guide](testing.md) separates source coverage, executable examples, automated tests and native observations. In particular, [input verification](../widget-input-verification.md) records scoped evidence and remaining final-native work. The [catalog](catalog.md) explicitly excludes promises such as general modal dialogs, UIA/accessibility integration and virtual collections.

## Sources and coverage

- [Public exports](../../crates/harbor-widget/src/lib.rs), [component/build context](../../crates/harbor-widget/src/view.rs), [construction and keys](../../crates/harbor-widget/src/construction.rs).
- [Store/action queue](../../crates/harbor-widget/src/store.rs), [signals](../../crates/harbor-widget/src/signal.rs), [native host](../../crates/harbor-widget/src/winit/host.rs).
- [Input requirements (0028)](../../.grimoire/spec/0028-reusable-single-line-text-input.md) and [glass/gradient/refraction requirements (0029)](../../.grimoire/spec/0029-search-panel-glass-appearance.md): requirements and remaining acceptance, not a claim that every scenario has passed.

All five guides above are part of this product line. API details remain grounded in this checkout's source; the comparisons to React and Flutter are explanatory only.

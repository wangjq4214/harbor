# Mental models: descriptions, identity and work

[WIDGET home](README.md) · [Learn](learn.md) · [Catalog](catalog.md)

Think in three separate lifetimes: **a description is rebuilt, a fiber is retained, and a host presents frames**. Conflating them leads to reset editors, stale subscriptions and windows that never wake.

## Translate ideas, not APIs

| Familiar idea | Harbor's model | Do not infer |
| --- | --- | --- |
| React render / Flutter widget build | `Component::build(&self, &mut BuildCx) -> View` describes UI | A DOM, Flutter engine, CSS or Material compatibility |
| Component identity / element identity | Retained fiber matched by compatible type and sibling-local key/position | State migration across parents or global keys |
| React hook state | Ordered `BuildCx::use_state` slots holding `Signal<T>` | React setter queues, render snapshots, automatic batching semantics |
| Flutter editing controller | `TextEditingController` owns coherent single-line editing state | Multiline, password or rich-text editing |
| React portal / Flutter overlay | `Portal` paints children in the root overlay while preserving logical ancestry | A modal dialog service, native window or automatic focus trap |
| Effects after render | Host executes `RuntimeEffects`; external leaves have explicit callbacks | `useEffect`, dependency arrays or generic lifecycle hooks |

The analogies help organize reasoning; they do not specify Harbor behavior.

## Identity determines local lifetime

A `View` is a short-lived description. A fiber retains hooks, subscriptions, children and dirty flags. A generational `FiberId` distinguishes a retained node from a later node reusing its arena slot; application code should normally use `FocusHandle`, not cache internal IDs as durable identity.

Reconciliation rules are parent-local:

1. A key unique in both old and new sibling lists may reuse a compatible type across sibling positions.
2. An unkeyed child reuses a compatible unkeyed child only at the same position.
3. Type/key mismatch mounts fresh state. Duplicate sibling keys produce diagnostics and do not reuse ambiguous identities.
4. Unmatched old fibers unmount and clean up subscriptions/input references.

Use `ComponentExt::keyed`, or `Component::key()` when implementing a component's identity directly. Reordering is supported; moving a keyed child to another parent is not a state-preservation contract. `Runtime::set_root` replaces and tears down the old root even when the new description looks similar.

**Construction matters:** passing a component to `.child(...)`/`view!` defers its build to its own fiber. Calling a child's `.build(cx)` eagerly shares the caller's build context. That can be intentional inside a composite implementation, but is not an independent hook boundary. `BuildCx::stub()` is for prebuilding descriptions; it is not a mounted runtime with persistent subscriptions.

Sources/coverage: [reconciliation](../../crates/harbor-widget/src/fiber/reconcile.rs), [construction](../../crates/harbor-widget/src/construction.rs), [keyed construction tests](../../crates/harbor-widget/tests/child_construction.rs).

## Signals are pull-based shared cells, not render snapshots

`Signal<T>` is a UI-thread `Rc<RefCell<_>>` handle. Clones refer to the same value. `read()` borrows the **current** value; it does not by itself subscribe the current component. Use `cx.track(&signal)` for an externally owned signal, `store.watch(cx)` for published application state, or `cx.use_state(...)` for a hook-created signal (which subscribes its fiber).

`set(value)` replaces immediately, increments the version and marks subscribed fibers dirty. Ordinary signals do this even for equal values. `Signal::new_distinct` suppresses equal replacements when `T: PartialEq`. Several writes before an update may coalesce dirty work; the next build pulls the latest value, not every intermediate value. This is not React snapshot or queued-updater semantics.

Release a `read()` guard before writing the same signal. A long-lived borrow across a callback or write can panic. A closure that captures a signal handle sees its latest value when it reads; a closure that captures a copied scalar has only that scalar. Be explicit about which behavior you need.

Signals are not a worker-thread messaging mechanism. Use a host channel/action dispatcher and a separate host wake for background intent; mutate UI state on the UI thread.

Sources/coverage: [signals and unit tests](../../crates/harbor-widget/src/signal.rs), [closure subscription test](../../crates/harbor-widget/tests/closure_component.rs), [Store subscription test](../../crates/harbor-widget/src/store.rs).

## Hooks and lifecycle: the exact public seams

The public state hook is `BuildCx::use_state<T: Clone + 'static>(init: impl FnOnce() -> T) -> Signal<T>`. The initializer runs when a new slot is created. Slots are retained by call order, so keep hook calls unconditional and type-stable. `BuildCx::track<T: 'static>(&Signal<T>)` tracks an external dependency; `theme()` reads the inherited theme. There is no public `use_effect`, `use_memo`, `use_ref`, `on_mount` or general component `dispose` hook.

Do not confuse these existing callbacks with a general hook system:

| Public seam | When/why to use it |
| --- | --- |
| `CustomPaint::on_unmount(Arc<ExternalUnmountFn>)`, where `ExternalUnmountFn = dyn Fn()` | Teardown of a retained external-paint leaf when it leaves the mounted tree |
| `CustomPaint::handler`, `schedule`, `ime`, `on_input` | External renderer, demand/deadline, focused IME and in-tree input adapters; not component mount effects |
| `BuildCx::register_external_draw`, `register_external_schedule`, `register_external_ime` | Per-build external registrations; ordinarily supplied through `CustomPaint` builders |
| `LayoutObserver::new(callback)` | Observe committed layout geometry; not a safe substitute for executing business work during build |
| `WindowPlatformHooks::configure_attributes`, `window_created`, `surface_ready` | Native host construction setup with host-lifetime retained setup state, not hooks on every component |

Keep long-lived resources with an explicit owner. If a root can be replaced or hot-reloaded, external registration/teardown and old callback lifetime require real integration coverage. A hook retaining a controller is not proof that controller state survives a root replacement.

Sources: [BuildCx](../../crates/harbor-widget/src/view.rs), [CustomPaint](../../crates/harbor-widget/src/widgets/custom_paint.rs), [native lifecycle API](../../crates/harbor-widget/src/winit/host.rs).

## Actions are intent, not state

`Store<S, A>` combines a state signal and a FIFO action inbox. `Dispatcher<A>` is cloneable and write-only; it can cross threads when its action type permits. It does not own a reducer or effect executor. The host/application drains actions at a safe event-turn boundary, decides what they mean, executes business effects and publishes state with `set_state`.

`Shortcuts<A>` maps key chords to typed actions; `Actions<A>` supplies a compatible ancestor handler. `ActionOutcome::Consumed` stops the triggering keyboard event, while `PassThrough` declines and allows routing to continue. These synchronous routing mechanisms are not the Store's FIFO queue. A handler can enqueue Store intent, but the host must still drain it.

In particular, **enqueuing does not automatically wake the runtime**. For background producers, sending an action and waking the native event loop are separate responsibilities. Even on a UI turn, arrange reduction/publication and runtime invalidation rather than assuming dispatch changes the screen.

Sources: [Store](../../crates/harbor-widget/src/store.rs), [Actions](../../crates/harbor-widget/src/widgets/actions.rs), [Shortcuts](../../crates/harbor-widget/src/widgets/shortcuts.rs). See the runnable [counter example](learn.md).

## Invalidation, effects and presentation are different steps

```text
input / background result
    → queue intent → application drains/reduces → publish state
    → mark runtime work → update/reconcile/layout/paint
    → RuntimeEffects + retained scene → host renders/presents
```

Build/layout/paint/hit-test dirty flags distinguish work categories. They are not a guarantee of isolated component execution: dirty build work currently can rebuild from the root. Layout-only invalidation is distinct from rebuilding components. Do not put business side effects in build based on an assumed number of builds.

`RuntimeEffects` describes redraw, waiting/deadlines, cursor, IME and clipboard requests. Core code does not execute native platform effects. `WinitWindowHost` applies them and manages surface outcomes; a custom host must implement the same responsibilities. Tagged editor clipboard requests complete via `Runtime::complete_clipboard`, not ordinary event dispatch.

A quiet host should wait, not continuously poll to hide missing wake/invalidation wiring. A paint request does not establish that an OS frame was shown, and a source/test check does not establish native acceptance. See [runtime architecture](../architecture/widget-runtime.md), [host recipes](recipes.md) and [testing](testing.md).

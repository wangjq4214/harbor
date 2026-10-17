# Learn WIDGET

[WIDGET home](README.md) · Next: [mental models](concepts.md)

Build a tree first, retain an editor second, then separate application intent from published state. Each Rust block below is independent and can be used as a small `main.rs` with a path dependency on `harbor-widget` (see the [crate setup](../../crates/harbor-widget/README.md)). These examples run headlessly: they do not open a window. For native hosting, continue to [recipes](recipes.md).

## 1. Describe an interface

A component receives a `BuildCx` and returns a `View`. Closures with the signature `Fn(&mut BuildCx) -> View + 'static` are components too. Build describes the interface; it is not the place to start I/O or execute application commands.

```rust
use harbor_widget::{Column, runtime::Runtime, view::{BuildCx, Component}};
use harbor_widget::widgets::text_label::TextLabel;
use std::time::Instant;

fn main() {
    let mut runtime = Runtime::new();
    runtime.set_root(|cx: &mut BuildCx| {
        Column::new().gap(8.0)
            .child(TextLabel::new("Hello, Harbor"))
            .child(TextLabel::new("A description, not a native control handle"))
            .build(cx)
    });
    let effects = runtime.update(Instant::now());
    assert!(effects.request_redraw);
}
```

`set_root` mounts and builds immediately; the first `update` collects pending work/effects. A redraw request is not a presented frame. The host must render and present the scene.

`.child(component)` defers that component's build until reconciliation assigns its fiber. The equivalent `view!` construction is shown in step 3. Prefer deferred construction for stateful children: eagerly calling each child's `.build(cx)` can put its hooks into the caller's slot sequence instead of giving it independent identity.

**Try:** add another label. Use `Row` instead of `Column`. Then read [flex layout](../flex-layout.md) before adding `Expanded` or scroll constraints: layout is parent-constrained, not CSS.

## 2. Keep editing state across builds

Do not recreate an editor controller on every build. Store its handle in a hook (or keep it in an application-owned object that outlives builds). Here the controller is initialized only once for the mounted component.

```rust
use harbor_widget::{TextEditingController, TextField, runtime::Runtime};
use harbor_widget::view::{BuildCx, Component, View};
use std::time::Instant;

struct Draft;
impl Component for Draft {
    fn build(&self, cx: &mut BuildCx) -> View {
        let retained = cx.use_state(|| TextEditingController::new("Hello"));
        let controller = retained.read().clone();
        TextField::new(controller)
            .placeholder("Write a greeting")
            .build(cx)
    }
}

fn main() {
    let mut runtime = Runtime::new();
    runtime.set_root(Draft);
    let _effects = runtime.update(Instant::now());
}
```

`use_state` returns a shared `Signal<T>`, not a React `[value, setter]` pair. Here the signal retains the controller; the controller owns coherent text, selection, composition and history. The editor subscribes to its internal editing state. There is no second string that must be copied back into the field each frame.

Keep hook calls in the same order and of the same types on every build. A type mismatch at the same slot panics. Unmounting discards component-local hook retention; replacing the root with `set_root` is a teardown, not a routine state update.

**Try in a native host:** type in the middle, select and replace text, and trigger an unrelated rebuild. The controller should not reset. Native behavior still requires the [input acceptance checks](testing.md); this headless snippet does not prove typing, clipboard or IME.

## 3. Send intent; publish the result

A button should queue application intent, not run a reducer during build. `Store<S, A>` has two distinct sides: `watch(cx)` subscribes to published state, and `dispatcher()` gives callbacks an action-only capability.

This complete example also supplies a tiny **application-owned** reducer. Calling the dispatcher in `main` simulates an action already delivered by a host turn; it does not simulate pointer input.

```rust
use harbor_widget::{Button, Column, Store, view};
use harbor_widget::view::{BuildCx, Component, View};
use harbor_widget::widgets::text_label::TextLabel;
use harbor_widget::runtime::Runtime;
use std::time::Instant;

#[derive(Clone, Copy)]
enum CounterAction { Increment }

#[derive(Clone)]
struct Counter { store: Store<u32, CounterAction> }

impl Component for Counter {
    fn build(&self, cx: &mut BuildCx) -> View {
        let count = *self.store.watch(cx); // end the borrow before callbacks run
        let actions = self.store.dispatcher();
        view! { cx; Column::new().gap(8.0) => {
            TextLabel::new(format!("Count: {count}"));
            Button::new("Increment")
                .on_click(move |_| actions.dispatch(CounterAction::Increment));
        } }
    }
}

fn reduce_pending(store: &Store<u32, CounterAction>) {
    let actions = store.drain_actions();
    if actions.is_empty() { return; }
    let mut next = *store.state().read();
    for action in actions {
        match action {
            CounterAction::Increment => next += 1,
        }
    }
    store.set_state(next);
}

fn main() {
    let store = Store::new(0);
    let mut runtime = Runtime::new();
    runtime.set_root(Counter { store: store.clone() });
    let _ = runtime.update(Instant::now());

    store.dispatcher().dispatch(CounterAction::Increment);
    assert_eq!(*store.state().read(), 0); // dispatch did not reduce anything
    reduce_pending(&store);              // application event-turn boundary
    assert_eq!(*store.state().read(), 1);
    assert!(runtime.update(Instant::now()).request_redraw);
    assert!(store.drain_actions().is_empty()); // intent was consumed once
}
```

`Button::on_click` requires `Fn(&mut EventCtx) + Send + Sync + 'static`. A UI-thread `Signal` uses `Rc<RefCell<_>>` and cannot be captured there. The action dispatcher is the appropriate capability when its action type satisfies the callback's thread bounds. Do not use `Arc<Mutex<Signal<_>>>` to bypass the UI-thread ownership model.

**Important:** `Dispatcher::dispatch` only enqueues. It does not wake a runtime, schedule a frame, run effects or publish state. Drain after input routing, reduce on the UI thread, publish, and ensure the host processes invalidation. Background senders also need an event-loop wake mechanism; an inbox alone cannot wake a sleeping window.

## 4. Give repeated children identity

Import `harbor_widget::ComponentExt` and use `.keyed(stable_id)` on components when items may reorder. For example, `.child(TextLabel::new("Saved").keyed("save-status"))` uses a stable sibling-local identity. Keys must be unique under their parent; do not generate a new key on every build or use changing display text as the ID.

Unkeyed children match compatible types at the same sibling position. Keyed children can preserve local state across sibling reordering, not across arbitrary parent moves. A changed type/key deliberately resets that identity. See [identity and hooks](concepts.md) for the rules and coverage links.

## Continue

- Choose controls from the [catalog](catalog.md), including the already-delivered text field, Portal and material primitives.
- Use [recipes](recipes.md) for native host lifecycle, controller callbacks and overlay composition.
- Use [testing](testing.md) to distinguish executable-example checks from real-window acceptance.

Source and coverage: [closure component test](../../crates/harbor-widget/tests/closure_component.rs), [construction/key tests](../../crates/harbor-widget/tests/child_construction.rs), [Store source/tests](../../crates/harbor-widget/src/store.rs), [input tests](../../crates/harbor-widget/tests/text_input.rs). Example verification status and commands are recorded in [testing](testing.md).

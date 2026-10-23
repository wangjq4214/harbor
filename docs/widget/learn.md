# Learn WIDGET

[WIDGET home](README.md) · Next: [mental models](concepts.md)

Build a tree first, retain an editor second, then separate application intent from published state. Each Rust block below is independent and can be used as a small `main.rs` with a path dependency on `harbor-widget` (see the [crate setup](../../crates/harbor-widget/README.md)). These examples run headlessly: they do not open a window. For native hosting, continue to [recipes](recipes.md).

## 1. Describe an interface

A component receives a `BuildCx` and returns a `View`. Closures with the signature `Fn(&mut BuildCx) -> View + 'static` are components too. `view!` also produces a `View`: it converts every root and child through `IntoChildView`, so components defer until reconciliation assigns their Fiber and no context parameter is passed to the macro. Build describes the interface; it is not the place to start I/O or execute application commands.

```rust
use harbor_widget::{Column, runtime::Runtime, view};
use harbor_widget::widgets::text_label::TextLabel;
use std::time::Instant;

fn main() {
    let mut runtime = Runtime::new();
    runtime.set_root(view! {
        Column::new().gap(8.0) => {
            TextLabel::new("Hello, Harbor");
            TextLabel::new("A description, not a native control handle");
        }
    });
    let effects = runtime.update(Instant::now());
    assert!(effects.request_redraw);
}
```

`set_root` accepts a component or a `view!` description; the root Fiber belongs to the Runtime and materializes the description during the first update, which collects pending work/effects. A redraw request is not a presented frame. The host must render and present the scene.

`.child(component)` defers that component's build until reconciliation assigns its fiber. The equivalent `view!` construction is shown in step 3. Prefer deferred construction for stateful children: eagerly calling each child's `.build(cx)` can put its hooks into the caller's slot sequence instead of giving it independent identity.

Child attachment is component-owned: leaves have no child block; single-child containers use `.child(value)` to replace their slot; `Row`, `Column`, `Flex`, and `Stack` use `.children(values)` to replace the list and `.push_child(value)` / `.extend_children(values)` to append. A `=> { ... }` block is one replacement, not an append. An empty block clears existing content; an ordinary value statement preserves it. `Align` owns one child (use `Stack` for overlays). `InteractiveRegion::child` and `.content` replace each other.

Static over-counts on single-child containers and all leaf blocks fail compilation. Runtime-dependent lists validate during construction: handwritten `.with_children(Children)` returns `ChildConstructionError`; ordinary `view!` panics with the receiver, allowed count, and incoming count. External components implement public `WithChildren` with `type Capability = SingleChild` or `MultiChild` and the same replacement/clearing rules; `Children::into_single` validates and replaces an `Option<View>` without building components.

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

**Try in a native host:** type in the middle, select and replace text, and trigger an unrelated rebuild. The controller should not reset.

## 3. Update local UI state directly

Ordinary UI callbacks can capture local Signals. Keep explicit clones for repeated builds and end read borrows before updating state. No action queue is needed for a purely local counter.

```rust
use harbor_widget::{Button, Column, runtime::Runtime, view};
use harbor_widget::view::{BuildCx, Component, View};
use harbor_widget::widgets::text_label::TextLabel;
use std::time::Instant;

struct LocalCounter;
impl Component for LocalCounter {
    fn build(&self, cx: &mut BuildCx) -> View {
        let count = cx.use_state(|| 0u32);
        cx.track(&count);
        let value = *count.read();
        view! { Column::new().gap(8.0) => {
            TextLabel::new(format!("Count: {value}"));
            Button::new("Increment").on_click(move |_| {
                let next = *count.read() + 1;
                count.set(next);
            });
        } }
    }
}

fn main() {
    let mut runtime = Runtime::new();
    runtime.set_root(LocalCounter);
    let _effects = runtime.update(Instant::now());
}
```

## 4. Send business intent; publish the result

For business operations, a button queues application intent rather than running a reducer or platform effect. `Store<S, A>` has two distinct sides: `watch(cx)` subscribes to published state, and `dispatcher()` gives callbacks an action-only capability.

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
        view! { Column::new().gap(8.0) => {
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

`Button::on_click` accepts `Fn(&mut EventCtx) + 'static` on the UI thread, including direct Signal or `Rc` captures. Use Dispatcher for business intent, not merely to satisfy callback bounds. Signal remains UI-thread-owned; do not wrap it in `Arc<Mutex<_>>` to share it with background work.

**Important:** `Dispatcher::dispatch` only enqueues. It does not wake a runtime, schedule a frame, run effects or publish state. Drain after input routing, reduce on the UI thread, publish, and ensure the host processes invalidation. Background senders also need an event-loop wake mechanism; an inbox alone cannot wake a sleeping window.

## 5. Give repeated children identity

Import `harbor_widget::ComponentExt` and use `.keyed(stable_id)` on components when items may reorder. For example, `.child(TextLabel::new("Saved").keyed("save-status"))` uses a stable sibling-local identity. Keys must be unique under their parent; do not generate a new key on every build or use changing display text as the ID.

Unkeyed children match compatible types at the same sibling position. Keyed children can preserve local state across sibling reordering, not across arbitrary parent moves. A changed type/key deliberately resets that identity. See [identity and hooks](concepts.md) for the rules.

## Continue

- Choose controls from the [catalog](catalog.md).
- Use [recipes](recipes.md) for native host lifecycle, controller callbacks and overlay composition.
- Use [testing](testing.md) to test components and troubleshoot native integration.

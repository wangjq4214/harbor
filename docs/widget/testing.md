# Testing components

[WIDGET home](README.md) · [Learn](learn.md) · [Recipes](recipes.md)

Use headless runtime tests for state, reconciliation, layout and routed events. Use a native host to check clipboard/IME, font metrics, DPI and presentation.

## A small deterministic test

This independent Rust example demonstrates explicit subscription and pull semantics. It records builds as test instrumentation, not as a pattern for business side effects in production build functions.

```rust
use harbor_widget::{runtime::Runtime, signal::Signal};
use harbor_widget::{BuildCx, Component};
use harbor_widget::TextLabel;
use std::{cell::RefCell, rc::Rc, time::Instant};

fn main() {
    let state = Signal::new(0_u32);
    let seen = Rc::new(RefCell::new(Vec::new()));
    let input = state.clone();
    let observations = seen.clone();
    let mut runtime = Runtime::new();
    runtime.set_root(move |cx: &mut BuildCx| {
        cx.track(&input);
        let value = *input.read();
        observations.borrow_mut().push(value);
        TextLabel::new(format!("{value}")).build(cx)
    });
    let _ = runtime.update(Instant::now());
    state.set(1);
    state.set(2);
    assert_eq!(*state.read(), 2); // immediate shared-cell value, before a build
    assert!(runtime.update(Instant::now()).request_redraw);
    assert_eq!(&*seen.borrow(), &[0, 2]); // build pulls latest value
}
```

For interaction tests, route `UiEvent` values through the runtime rather than calling an action dispatcher directly. Exercise constraints and resize, focus traversal and disabled behavior, Unicode selection and editing, and overlay clipping and dismissal as applicable.

## Run guide examples

From the repository root:

```bash
python scripts/docs/check_examples.py --include-readmes
```

This compiles and runs Markdown examples against the workspace path dependencies in an isolated Cargo harness under `target/docs-examples`. `cargo test -p harbor-widget --doc` alone does not include these guides. The native event-loop example is marked `rust,no_run`: it is compiled but does not open a window. Headless examples do not exercise native clipboard, IME or GPU presentation.

For native components, also check real typing, IME composition/cancellation, clipboard, resize and fractional DPI. For backdrop filters, test over changing content and check that foreground text stays sharp.

## Diagnose by boundary

- **State changed, nothing rebuilds:** was the external signal tracked or Store watched? Did queued actions get drained and published? Did a background producer wake the host?
- **Editor resets:** did a build create a fresh controller, change key/type/parent, or call `set_root` instead of publishing state?
- **Overlay looks right but focus is wrong:** Portal does not supply initial focus, dismissal or modality. Verify handle ownership and routing explicitly.
- **Headless geometry passes, native caret is wrong:** inspect actual text resources, DPI and host IME positioning; synthetic fallback metrics are not native typography proof.
- **Glass looks like transparency only:** verify real changing backdrop content, correct paint order and the full frame encoding path; a uniform background is a poor blur diagnostic.

See the [input](../widget-input.md) and [flex layout](../flex-layout.md) contracts.

# Test descriptions, then verify the native experience

[WIDGET home](README.md) · [Learn](learn.md) · [Recipes](recipes.md)

A source implementation, a compiled example, a headless assertion and a native observation answer different questions. Record which one you have. None is a substitute for the others.

## Evidence ladder

| Check | Establishes | Does not establish |
| --- | --- | --- |
| Source/API inspection | A named public type/method and its current contract exist | That an example compiles or a runtime scenario passes |
| Rustdoc compilation | Imports, trait bounds, signatures and macro use type-check | Successful presentation, real clipboard/IME, visual quality |
| Headless runtime test | Deterministic state, reconciliation, layout or event behavior under supplied inputs | Native event ordering, real DPI/font metrics or candidate-window behavior |
| GPU/offscreen test | Particular encoded paint/frame behavior on the tested backend | Full native-host lifecycle or cross-backend visual acceptance |
| Native acceptance | Observed behavior on a recorded binary/environment/scenario | Untested scenarios/platforms or a stable public API |

## A small deterministic test

This independent Rust example demonstrates explicit subscription and pull semantics. It records builds as test instrumentation, not as a pattern for business side effects in production build functions.

```rust
use harbor_widget::{runtime::Runtime, signal::Signal};
use harbor_widget::view::{BuildCx, Component};
use harbor_widget::widgets::text_label::TextLabel;
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

Extend this style with existing routed `UiEvent` tests when testing interaction. Do not claim pointer/keyboard coverage by calling an action dispatcher directly. Use a native test for native behavior, not a headless approximation labelled end-to-end.

## Widget acceptance matrix

Every delivered interactive widget needs proportionate evidence for the applicable rows below. These are widget-specific outcomes, not a second shared command/release policy; use [validation](../validation.md) for quality gates and evidence recording. [Remaining toolkit work](catalog.md#remaining-toolkit-work) retains dependency tiers and representative exit workflows.

| Area | Evidence to retain |
| --- | --- |
| Layout | Loose/tight constraints, minimum/maximum and zero sizes, overflow, resize and scale changes. |
| Interaction | Capture/cancel, hover transitions, primary/secondary buttons where relevant, keyboard activation, focus traversal, disabled behavior and shortcut conflicts. |
| Text input | Unicode boundaries, selection, clipboard, undo/redo, IME preedit/commit, focus loss and candidate positioning. |
| Overlays | Clipping, z-order, outside click, Escape, modal routing, focus restoration and placement near every window edge. |
| Accessibility | Stable role/name/value/state/action output and keyboard-only completion of the representative workflow; Windows UIA smoke once the bridge exists. Keyboard support alone is not a semantic tree. |
| Rendering | Retained scene identity, transparent paint order, clipping, high DPI, practical GPU encode coverage and visual/native observations across control states. |
| Performance | No idle redraw, bounded invalidation and virtualization for unbounded collections; measurements before introducing caches or spatial indexes. |
| Documentation | A catalog example and public behavior notes per control family; source/test links and the shared language/link checks. |

## Existing coverage to reuse

| Area | Source/test entry points |
| --- | --- |
| Hook state and subscriptions | [closure components](../../crates/harbor-widget/tests/closure_component.rs), [signals](../../crates/harbor-widget/src/signal.rs), [Store FIFO/published-state tests](../../crates/harbor-widget/src/store.rs) |
| Identity and construction | [child construction](../../crates/harbor-widget/tests/child_construction.rs), [component boundaries / Portal](../../crates/harbor-widget/tests/component_boundaries.rs) |
| Layout and scroll geometry | [flex runtime](../../crates/harbor-widget/tests/flex_runtime.rs), [layout observer](../../crates/harbor-widget/tests/layout_observer.rs), [scroll area](../../crates/harbor-widget/tests/scroll_area.rs) |
| Focus/actions/interaction/theme | [interaction suite](../../crates/harbor-widget/tests/interaction_focus_actions_theme.rs), [input routing](../../crates/harbor-widget/tests/input_routing.rs) |
| Text, history, clipboard eligibility, IME sequences | [controller unit tests](../../crates/harbor-widget/src/text_editing.rs), [text input integration](../../crates/harbor-widget/tests/text_input.rs), [native-adapter effects](../../crates/harbor-widget/src/winit/effects.rs) |
| Decoration, gradients, backdrop/refraction | [decoration](../../crates/harbor-widget/tests/decorated_box.rs), [paint validation](../../crates/harbor-widget/src/paint.rs), [filter unit tests](../../crates/harbor-widget/src/widgets/backdrop_filter.rs), [quad/frame GPU tests](../../crates/harbor-widget/tests/quad_renderer.rs) |
| Native host lifecycle | [ignored native smoke](../../crates/harbor-widget/tests/native_host_smoke.rs), [winit contracts](../../crates/harbor-widget/tests/winit_contracts.rs), [winit QA](../../crates/harbor-widget/tests/winit_qa.rs) |

These links locate coverage; they do not claim those tests were rerun for a documentation edit.

## Commands and example verification

From the repository root, select checks appropriate to the change:

```bash
cargo test --locked -p harbor-widget --test closure_component --test child_construction
cargo test --locked -p harbor-widget --test text_input --test component_boundaries
cargo test --locked -p harbor-widget --features winit
cargo test --locked -p harbor-widget --doc
```

The crate's normal `--doc` command does **not** automatically include this product line's Markdown. Run `python scripts/docs/check_examples.py --include-readmes` for the repeatable guide/crate-README harness. It generates an isolated Cargo library under `target/docs-examples`, includes pages through Rustdoc, seeds a derived lock from the workspace pins, and compiles/executes the appropriate snippets against actual path dependencies. Product manifests and the root lockfile remain unchanged. See [documentation maintenance](../README.md#rust-examples-and-api-reference) for selection and limits.

Example constraints:

- All Rust fences in [Learn](learn.md) are independent, headless executables.
- [Recipes](recipes.md) marks the native event-loop example `rust,no_run`: compile it, but do not launch a window in an unattended documentation test. Its other two examples are headless; the glass example constructs a tree without GPU rendering.
- The signal example above is headless. Diagrams/conceptual flows use `text`, not falsely runnable Rust.
- Rustdoc execution is a repeatable automated gate, not native acceptance. `no_run` verifies compilation only. No workspace-wide suite, clippy, visual, clipboard or interactive IME acceptance follows from a passing guide-example run.

To run the existing native host smoke explicitly on an interactive Windows desktop with a compatible GPU:

```bash
cargo test --locked -p harbor-widget --features winit --test native_host_smoke -- --ignored
```

This hidden-window presentation/lifecycle test is not a field-typing, Chinese IME or material-appearance acceptance test. A skipped/ignored test is not a pass. Follow [validation policy](../validation.md) for broader gates and evidence recording.

## Remaining native acceptance

The [input verification record](../widget-input-verification.md) contains prior deterministic coverage and a limited native Latin/selection/clipboard smoke. That smoke preceded final refinements and is not final integrated acceptance. [Spec 0028](../../.grimoire/spec/0028-reusable-single-line-text-input.md) still calls for actual Chinese candidate/cancel/commit ordering, mixed-Unicode appearance/caret, real resize/DPI/drag, supported live DLL/HMR replacement and extended terminal coexistence/lifecycle checks.

[Spec 0029](../../.grimoire/spec/0029-search-panel-glass-appearance.md) requires native observations of local blur over changing terminal content, sharp foreground, gradient/alpha/clipping, edge/corner displacement fading to a stable center, and resize/fractional-DPI behavior. It also requires the application search composition/status/navigation behavior; a generic widget example cannot accept that application policy on its behalf. Implementation and automated sampling tests do not constitute a fresh visual pass.

For each native run, record revision and dirty scope, target/backend/OS/DPI/font environment, steps, expected versus observed behavior, artifacts and exclusions, and a result such as PASS, FAIL, NOT RUN or BLOCKED. Keep clipboard/input payloads out of logs and evidence. Do not introduce a new performance threshold or claim an improvement without measurements.

## Diagnose by boundary

- **State changed, nothing rebuilds:** was the external signal tracked or Store watched? Did queued actions get drained and published? Did a background producer wake the host?
- **Editor resets:** did a build create a fresh controller, change key/type/parent, or call `set_root` instead of publishing state?
- **Overlay looks right but focus is wrong:** Portal does not supply initial focus, dismissal or modality. Verify handle ownership and routing explicitly.
- **Headless geometry passes, native caret is wrong:** inspect actual text resources, DPI and host IME positioning; synthetic fallback metrics are not native typography proof.
- **Glass looks like transparency only:** verify real changing backdrop content, correct paint order and the full frame encoding path; a uniform background is a poor blur diagnostic.

Use [runtime architecture](../architecture/widget-runtime.md), [input](../widget-input.md) and [flex layout](../flex-layout.md) for the corresponding contracts rather than inventing parallel mechanisms.

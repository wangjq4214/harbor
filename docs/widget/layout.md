# Explicit layout and fallible nodes

[WIDGET home](README.md) · [Catalog](catalog.md) · [Parent-directed flex contract](../flex-layout.md)

Layout is an ordinary component tree, not a style bag or a set of general `ComponentExt` wrapping methods. Keep `.keyed(...)`, Button callbacks and TextField editing configuration local to their component. External `Padding` is distinct from TextField's internal field padding.

## Declare the tree

Ordinary `view!` accepts either a component/existing View or a final `Result` containing one. No per-node question mark or unwrap is required, including nodes with child blocks, selected if/match branches and loop-produced children:

```rust
use harbor_widget::{Align, Alignment, Alignment2D, Button, Column, Expanded,
    FlexOptions, Insets, Padding, Row, SizedBox, TextEditingController, TextField, view};
use harbor_widget::widgets::text_label::TextLabel;

fn main() {
    let controller = TextEditingController::new("");
    let tree = view! {
        Align::new(Alignment2D::TOP_RIGHT) => {
            SizedBox::width(440.0) => {
                Padding::new(Insets::all(8.0)) => {
                    Column::with_options(FlexOptions { gap: 8.0, ..Default::default() }) => {
                        TextLabel::new("Search");
                        Row::with_options(FlexOptions {
                            gap: 4.0,
                            cross_axis_alignment: Alignment::Center,
                            ..Default::default()
                        }) => {
                            Expanded::new() => { TextField::new(controller); }
                            Button::text("Close").on_click(|_| {});
                        }
                    }
                }
            }
        }
    };
    let mut runtime = harbor_widget::runtime::Runtime::new();
    runtime.set_root(tree);
    runtime.update(std::time::Instant::now());
}
```

Expressions evaluate once, siblings in source order and children before their parent expression. Unselected branches stay lazy; describing a successful component does not build it. Err panics at construction with the actual error and originating node's file/line/column. This is not a fallible Runtime or a rollback/recovery guarantee.

## Recover in handwritten Rust

Requested dimensions, gaps and every individual inset side must be finite and nonnegative (zero is valid). Constructors and raw-numeric setters return `LayoutConfigError`. `Insets` and `FlexOptions` are plain input values, not proof of validation:

```rust
use harbor_widget::{Insets, LayoutConfigError, Padding, SizedBox, view};
use harbor_widget::widgets::text_label::TextLabel;

fn padded(width: f32) -> Result<Padding, LayoutConfigError> {
    let sized = SizedBox::width(width)?.child(TextLabel::new("content"));
    Ok(Padding::new(Insets { top: 2.0, right: 4.0, bottom: 2.0, left: 4.0 })?
        .child(sized))
}
fn main() {
    assert!(padded(f32::INFINITY).is_err());
    let valid = padded(-1.0).unwrap_or_else(|_| padded(200.0).unwrap());
    let _ = view! { valid; };
}
```

Only the **final** expression result is resolved. `SizedBox::width(100.0).child(...)` is not a valid Rust chain: first handle the Result outside the macro, use `map`, or use `SizedBox::width(100.0) => { ... }`. Use local options rather than long fallible chains. Flex defaults remain zero gap, Start main alignment and Start cross alignment; `Flex::with_options(axis, options)` also requires an explicit axis. Row and Column keep their own identities and the same underlying engine, including cross-axis Stretch.

Scalar validity is separate from geometry: two opposing `f32::MAX` inset sides construct successfully; their sum fails during layout. Valid parent maxima may be infinite. Layout errors retain the existing atomic geometry-commit behavior; constructors do not turn those errors into macro panics.

## Size, slots and wrapper order

- `SizedBox::new(Size)` requests both axes; `width(f32)`/`height(f32)` request only one. `requested_width()`/`requested_height()` return `Option<f32>`; omission is not zero or infinity. Configuration is private.
- Each requested extent is clamped to the authoritative parent interval and passed tightly to the child. Width 440 becomes 300 under a 300 cap or 500 under tight width 500. Omitted axes inherit parent constraints and follow child natural size, or zero without a child, then satisfy parent minima. Children sit at the origin; color paints the allocated rectangle, without implicit clipping or a focus stop.
- SizedBox, Padding and Align have optional SingleChild slots. Repeated `.child(...)` replaces. An attached macro block replaces staged content; `{}` clears it. Dynamic overcount returns `ChildConstructionError` in handwritten `WithChildren`; ordinary macro attachment panics. Statically provable overcount fails compilation. True leaves (for example TextLabel) reject every block, even an empty one or a final `Ok(leaf)`.
- Align uses positioning-only `Alignment2D` (`TOP_RIGHT`, `CENTER`, or explicit horizontal/vertical positions), not flex Stretch. `.insets(Insets)` is fallible and reduces both child constraints and available positioning space. Finite bounded axes fill; unbounded axes follow natural extent. Empty alignment space is not a hit target.
- Padding changes child constraints, offset and its own/background size. Decoration and clip wrappers keep their declared order. `Padding => DecoratedBox` insets the painted/clipped box; `DecoratedBox => Padding` paints/clips around the padding too. Do not reorder wrappers based only on equal outer dimensions, or mechanically replace Align's insets with another wrapper order.
- `Expanded`/`Flexible` metadata belongs to its **immediate** Flex/Row/Column parent. `Row => Expanded => Padding` works; `Row => Padding => Expanded` is misuse, not metadata tunneling.
- `ConstrainedBox` remains for real ranges, caps and current-font-metric-dependent `max_height_with`. Fixed width plus a maximum height is not a two-axis fixed size.

Keys retain identity only for stable parent-local key/type relationships. Inserting a wrapper or changing its type may remount descendants; keys do not promise preservation across arbitrary tree rewrites. Native focus/IME and actual HMR teardown still require separate host evidence.

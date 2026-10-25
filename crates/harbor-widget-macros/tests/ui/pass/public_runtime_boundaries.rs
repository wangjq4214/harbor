use harbor_widget::{
    ComponentExt, IntoChildView, KeyChord, Padding, Store,
    input::event::{Key, Modifiers},
    layout::{BoxConstraints, Rect, Size},
    runtime::Runtime,
    scene::primitive::Primitive,
    signal::Signal,
    text::TextMetrics,
    view::{BuildCx, PrimitiveLeaf, View},
};

struct Leaf;
impl PrimitiveLeaf for Leaf {
    fn intrinsic_size(&self, constraints: BoxConstraints, _: &TextMetrics) -> Size {
        constraints.constrain(Size::ZERO)
    }
    fn paint_primitives(&self, _: Rect, _: &TextMetrics) -> Vec<Primitive> {
        vec![]
    }
}

fn main() {
    let chord = KeyChord::new(Key::Character('A'), Modifiers::default());
    let _ = (chord.key(), chord.modifiers());
    let signal = Signal::new(1u32);
    let store = Store::<u32, ()>::new(2);
    let root = move |cx: &mut BuildCx| {
        cx.track(&signal);
        let _ = signal.read();
        let _ = store.watch(cx);
        let _ = cx.use_state(|| 0u32);
        harbor_widget::view! {
            Padding::all(1.0).unwrap().keyed("root") => {
                View::leaf(Leaf);
            }
        }
    };
    let mut runtime = Runtime::new();
    runtime.set_root(root.into_child_view());
    runtime.update(std::time::Instant::now());
    let _ = runtime.arena();
}

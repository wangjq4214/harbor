use harbor_widget::{
    KeyChord,
    input::event::{Key, Modifiers},
};

fn main() {
    let _ = KeyChord {
        key: Key::Character('A'),
        modifiers: Modifiers::default(),
    };
}

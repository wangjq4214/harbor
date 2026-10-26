use harbor_widget::{
    KeyChord,
    input::event::{Key, Modifiers},
};

fn main() {
    let mut chord = KeyChord::new(Key::Character('A'), Modifiers::default());
    let _ = chord.key;
    let _ = chord.modifiers;
    chord.key = Key::Character('B');
    chord.modifiers = Modifiers::default();
}

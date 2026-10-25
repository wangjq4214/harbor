//! Public value and mounted routing contracts for normalized shortcut chords.
use std::{cell::RefCell, collections::HashSet, rc::Rc, time::Instant};

use harbor_widget::{
    ActionOutcome, Actions, Focus, KeyChord, Shortcuts,
    input::{
        event::{Key, KeyboardEvent, Modifiers, UiEvent},
        event_ctx::{EventCtx, EventHandled},
    },
    layout::{BoxConstraints, Rect, Size},
    runtime::Runtime,
    scene::primitive::Primitive,
    text::TextMetrics,
    view::{PrimitiveLeaf, View},
};

#[test]
fn normalized_values_preserve_modifiers_key_variants_and_hash_equality() {
    for bits in 0..16 {
        let modifiers = Modifiers {
            shift: bits & 1 != 0,
            ctrl: bits & 2 != 0,
            alt: bits & 4 != 0,
            meta: bits & 8 != 0,
        };
        for (upper, lower) in [
            (Key::Character('A'), Key::Character('a')),
            (Key::NumpadCharacter('A'), Key::NumpadCharacter('a')),
        ] {
            let chord = KeyChord::new(upper, modifiers);
            assert_eq!(chord.key(), lower);
            assert_eq!(chord.modifiers(), modifiers);
            let lower = KeyChord::new(lower, modifiers);
            assert_eq!(chord, lower);
            let mut unique = HashSet::new();
            unique.insert(chord);
            assert!(!unique.insert(lower), "equal values must hash equally");
        }
        for key in [
            Key::Character('É'),
            Key::NumpadCharacter('É'),
            Key::F6,
            Key::Enter,
            Key::NumpadEnter,
        ] {
            assert_eq!(KeyChord::new(key, modifiers).key(), key);
        }
        assert_ne!(
            KeyChord::new(Key::Character('É'), modifiers),
            KeyChord::new(Key::Character('é'), modifiers)
        );
        assert_ne!(
            KeyChord::new(Key::Character('a'), modifiers),
            KeyChord::new(Key::NumpadCharacter('a'), modifiers)
        );
        assert_ne!(
            KeyChord::new(Key::Enter, modifiers),
            KeyChord::new(Key::NumpadEnter, modifiers)
        );
        for other in 0..16 {
            let other = Modifiers {
                shift: other & 1 != 0,
                ctrl: other & 2 != 0,
                alt: other & 4 != 0,
                meta: other & 8 != 0,
            };
            assert_eq!(
                KeyChord::new(Key::Character('A'), modifiers)
                    == KeyChord::new(Key::Character('a'), other),
                modifiers == other
            );
        }
    }
}

struct KeySink(Rc<RefCell<Vec<(Key, Modifiers)>>>);

impl PrimitiveLeaf for KeySink {
    fn intrinsic_size(&self, constraints: BoxConstraints, _: &TextMetrics) -> Size {
        constraints.constrain(Size::new(20.0, 20.0))
    }

    fn paint_primitives(&self, _: Rect, _: &TextMetrics) -> Vec<Primitive> {
        vec![]
    }

    fn handle_event(&self, event: &UiEvent, _: &mut EventCtx, _: Rect) -> EventHandled {
        if let UiEvent::Keyboard(KeyboardEvent::KeyDown { key, modifiers }) = event {
            self.0.borrow_mut().push((*key, *modifiers));
        }
        EventHandled::Ignored
    }
}

fn press(runtime: &mut Runtime, key: Key, modifiers: Modifiers) {
    runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown { key, modifiers }));
    runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyUp { key, modifiers }));
}

#[test]
fn mounted_chords_normalize_case_but_keep_exact_modifiers_and_keypad_identity() {
    for (bound, matching, other_variant) in [
        (
            Key::Character('A'),
            Key::Character('a'),
            Key::NumpadCharacter('a'),
        ),
        (
            Key::NumpadCharacter('A'),
            Key::NumpadCharacter('a'),
            Key::Character('a'),
        ),
    ] {
        for outcome in [ActionOutcome::Consumed, ActionOutcome::PassThrough] {
            let modifiers = Modifiers {
                ctrl: true,
                shift: true,
                alt: true,
                meta: true,
            };
            let actions = Rc::new(RefCell::new(Vec::new()));
            let delivered = Rc::new(RefCell::new(Vec::new()));
            let observed = actions.clone();
            let mut runtime = Runtime::new();
            runtime.set_root(Actions::new(
                Shortcuts::new(Focus::new(View::leaf(KeySink(delivered.clone()))))
                    .bind(KeyChord::new(bound, modifiers), 7u8),
                move |action: u8| {
                    observed.borrow_mut().push(action);
                    outcome
                },
            ));
            runtime.update(Instant::now());
            assert!(runtime.focus_first_focusable());

            press(&mut runtime, bound, modifiers);
            press(&mut runtime, matching, modifiers);
            assert_eq!(*actions.borrow(), [7, 7]);
            assert_eq!(
                delivered.borrow().len(),
                if outcome == ActionOutcome::Consumed {
                    0
                } else {
                    2
                }
            );
            delivered.borrow_mut().clear();

            // Every modifier is significant, including Shift after case folding.
            for mismatch in [
                Modifiers {
                    ctrl: false,
                    ..modifiers
                },
                Modifiers {
                    shift: false,
                    ..modifiers
                },
                Modifiers {
                    alt: false,
                    ..modifiers
                },
                Modifiers {
                    meta: false,
                    ..modifiers
                },
            ] {
                press(&mut runtime, matching, mismatch);
            }
            press(&mut runtime, other_variant, modifiers);
            assert_eq!(
                *actions.borrow(),
                [7, 7],
                "unmatched keys must not invoke the action"
            );
            assert_eq!(
                delivered.borrow().len(),
                5,
                "unmatched keys retain focused delivery"
            );
            assert_eq!(delivered.borrow()[4], (other_variant, modifiers));
        }
    }
}

#[test]
fn duplicate_normalized_bindings_choose_the_first_action() {
    for outcome in [ActionOutcome::Consumed, ActionOutcome::PassThrough] {
        let modifiers = Modifiers {
            ctrl: true,
            ..Modifiers::default()
        };
        let actions = Rc::new(RefCell::new(Vec::new()));
        let observed = actions.clone();
        let delivered = Rc::new(RefCell::new(Vec::new()));
        let mut runtime = Runtime::new();
        runtime.set_root(Actions::new(
            Shortcuts::new(Focus::new(View::leaf(KeySink(delivered.clone()))))
                .bind(KeyChord::new(Key::Character('A'), modifiers), 1u8)
                .bind(KeyChord::new(Key::Character('a'), modifiers), 2u8),
            move |action: u8| {
                observed.borrow_mut().push(action);
                outcome
            },
        ));
        runtime.update(Instant::now());
        assert!(runtime.focus_first_focusable());
        press(&mut runtime, Key::Character('a'), modifiers);
        assert_eq!(*actions.borrow(), [1]);
        assert_eq!(
            delivered.borrow().len(),
            usize::from(outcome == ActionOutcome::PassThrough)
        );
    }
}

#[test]
fn nested_shortcuts_still_choose_the_nearest_map_and_action_handler() {
    let chord = KeyChord::new(Key::Character('A'), Modifiers::default());
    let actions = Rc::new(RefCell::new(Vec::new()));
    let outer = actions.clone();
    let inner = actions.clone();
    let mut runtime = Runtime::new();
    runtime.set_root(Actions::new(
        Shortcuts::new(Actions::new(
            Shortcuts::new(Focus::new(View::leaf(KeySink(Default::default())))).bind(chord, 2u8),
            move |action: u8| {
                inner.borrow_mut().push(("inner", action));
                ActionOutcome::Consumed
            },
        ))
        .bind(chord, 1u8),
        move |action: u8| {
            outer.borrow_mut().push(("outer", action));
            ActionOutcome::Consumed
        },
    ));
    runtime.update(Instant::now());
    assert!(runtime.focus_first_focusable());
    press(&mut runtime, Key::Character('a'), Modifiers::default());
    assert_eq!(*actions.borrow(), [("inner", 2)]);
}

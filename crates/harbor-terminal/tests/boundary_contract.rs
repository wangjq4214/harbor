//! External contract tests for terminal-owned boundary types.
//!
//! Imports only public `harbor_terminal` types — no `harbor_widget` dependency.

use harbor_terminal::{
    Background, CellAttrs, Color, RenderTarget, RenderViewport, Terminal, TerminalAppearance,
    TerminalEvent, TerminalFocusEvent, TerminalKey, TerminalKeyboardEvent, TerminalModifiers,
    TerminalPointerButton, TerminalPointerEvent, TerminalPointerPhase,
    alpha_mode_supports_transparency,
};

#[test]
fn harbor_terminal_manifest_does_not_depend_on_harbor_widget() {
    let manifest = include_str!("../Cargo.toml");
    assert!(
        !manifest.lines().any(|line| {
            let trimmed = line.trim();
            trimmed.starts_with("harbor-widget") || trimmed.contains("harbor-widget =")
        }),
        "harbor-terminal must not declare a harbor-widget dependency"
    );
}

// ── RenderTarget ────────────────────────────────────────────────────────────

#[test]
fn should_preserve_geometry_fields_when_constructed() {
    // Arrange
    let origin = (12.5, 24.0);
    let allocation = (800, 600);
    let surface = (1920, 1080);

    // Act
    let target = RenderTarget::new(origin, allocation, surface);

    // Assert
    assert_eq!(target.allocation_origin, origin);
    assert_eq!(target.allocation_size, allocation);
    assert_eq!(target.surface_size, surface);
}

#[test]
fn should_compare_equal_when_geometry_matches() {
    // Arrange
    let left = RenderTarget::new((12.5, 24.0), (800, 600), (1920, 1080));
    let right = RenderTarget::new((12.5, 24.0), (800, 600), (1920, 1080));

    // Act / Assert
    assert_eq!(left, right);
}

#[test]
fn should_compare_unequal_when_origin_differs() {
    // Arrange
    let base = RenderTarget::new((12.5, 24.0), (800, 600), (1920, 1080));
    let other = RenderTarget::new((0.0, 0.0), (800, 600), (1920, 1080));

    // Act / Assert
    assert_ne!(base, other);
}

#[test]
fn should_preserve_zero_sizes_and_negative_fractional_origin_when_constructed() {
    // Arrange
    let origin = (-3.25, 0.5);
    let allocation = (0, 0);
    let surface = (0, 1080);

    // Act
    let target = RenderTarget::new(origin, allocation, surface);

    // Assert
    assert_eq!(target.allocation_origin, origin);
    assert_eq!(target.allocation_size, allocation);
    assert_eq!(target.surface_size, surface);
}

// ── TerminalAppearance / background rendering ──────────────────────────────

#[test]
fn should_render_explicit_ansi_background_as_an_opaque_fill() {
    // Arrange
    let mut terminal = Terminal::new_headless(1, 1);
    terminal.put_str("\x1b[41mX\x1b[0m");
    let cell = terminal.screen().cell(0, 0);
    let snapshot = terminal.screen().terminal_snapshot();
    let viewport = RenderViewport::new(10.0, 20.0);

    // Act
    let vertices = Background::build_background_row_vertices(0, &snapshot, &viewport);

    // Assert
    assert!(!cell.attrs.contains(CellAttrs::INVERSE));
    assert_eq!(cell.bg, Color::Named(1));
    assert_eq!(vertices[0].color, Color::Named(1).to_rgba());
    assert_eq!(vertices[0].color[3], 1.0);
}

#[test]
fn should_keep_default_color_fully_opaque() {
    assert_eq!(Color::Default.to_rgba(), [1.0, 1.0, 1.0, 1.0]);
}

#[test]
fn should_keep_ansi_color_fully_opaque() {
    assert_eq!(Color::Named(1).to_rgba()[3], 1.0);
}

#[test]
fn should_render_inverse_default_background_as_an_opaque_foreground_fill() {
    // Arrange
    let mut terminal = Terminal::new_headless(1, 1);
    terminal.put_str("\x1b[7mX\x1b[0m");
    let cell = terminal.screen().cell(0, 0);
    let snapshot = terminal.screen().terminal_snapshot();
    let viewport = RenderViewport::new(10.0, 20.0);

    // Act
    let vertices = Background::build_background_row_vertices(0, &snapshot, &viewport);

    // Assert
    assert!(cell.attrs.contains(CellAttrs::INVERSE));
    assert_eq!(vertices[0].color, Color::Default.to_rgba());
    assert_eq!(vertices[0].color[3], 1.0);
}

#[test]
fn should_use_terminal_tint_with_backdrop_and_opaque_rgb_without_it() {
    // Arrange
    let appearance = TerminalAppearance::new([0.36, 0.20, 0.08, 0.25]);

    // Act / Assert
    assert_eq!(appearance.rgba(), [0.36, 0.20, 0.08, 0.25]);
    assert_eq!(appearance.clear_rgba(true), [0.36, 0.20, 0.08, 0.25]);
    assert_eq!(appearance.clear_rgba(false), [0.36, 0.20, 0.08, 1.0]);
}

#[test]
fn should_use_configured_background_for_default_terminal_appearance() {
    assert_eq!(
        TerminalAppearance::default().rgba(),
        harbor_config::BACKGROUND
    );
}

#[test]
fn should_only_support_premultiplied_alpha_for_transparent_terminal_frames() {
    assert!(alpha_mode_supports_transparency(
        wgpu::CompositeAlphaMode::PreMultiplied
    ));
    for mode in [
        wgpu::CompositeAlphaMode::PostMultiplied,
        wgpu::CompositeAlphaMode::Auto,
        wgpu::CompositeAlphaMode::Inherit,
        wgpu::CompositeAlphaMode::Opaque,
    ] {
        assert!(!alpha_mode_supports_transparency(mode), "mode={mode:?}");
    }
}

// ── TerminalEvent ───────────────────────────────────────────────────────────

#[test]
fn should_treat_keyboard_pointer_and_focus_as_structurally_distinct() {
    // Arrange
    let keyboard = TerminalEvent::Keyboard(TerminalKeyboardEvent::KeyDown {
        key: TerminalKey::Tab,
        modifiers: TerminalModifiers::default(),
    });
    let pointer = TerminalEvent::Pointer(TerminalPointerEvent::new(
        (0.0, 0.0),
        TerminalPointerPhase::Up,
        TerminalPointerButton::Middle,
        1,
    ));
    let focus = TerminalEvent::Focus(TerminalFocusEvent::Gained);

    // Act / Assert
    assert_ne!(keyboard, pointer);
    assert_ne!(pointer, focus);
    assert_ne!(keyboard, focus);
}

#[test]
fn should_compare_equal_when_nested_payloads_match() {
    // Arrange
    let left = TerminalEvent::Focus(TerminalFocusEvent::Gained);
    let right = TerminalEvent::Focus(TerminalFocusEvent::Gained);

    // Act / Assert
    assert_eq!(left, right);
}

#[test]
fn should_compare_unequal_when_nested_focus_payload_differs() {
    // Arrange
    let gained = TerminalEvent::Focus(TerminalFocusEvent::Gained);
    let lost = TerminalEvent::Focus(TerminalFocusEvent::Lost);

    // Act / Assert
    assert_ne!(gained, lost);
}

#[test]
fn should_compare_equal_when_keyboard_payload_matches() {
    // Arrange
    let modifiers = TerminalModifiers {
        ctrl: true,
        ..TerminalModifiers::default()
    };
    let left = TerminalEvent::Keyboard(TerminalKeyboardEvent::KeyDown {
        key: TerminalKey::Enter,
        modifiers,
    });
    let right = TerminalEvent::Keyboard(TerminalKeyboardEvent::KeyDown {
        key: TerminalKey::Enter,
        modifiers,
    });

    // Act / Assert
    assert_eq!(left, right);
}

#[test]
fn should_compare_equal_when_pointer_payload_matches() {
    // Arrange
    let left = TerminalEvent::Pointer(TerminalPointerEvent::new(
        (1.0, 2.0),
        TerminalPointerPhase::Move,
        TerminalPointerButton::Left,
        9,
    ));
    let right = TerminalEvent::Pointer(TerminalPointerEvent::new(
        (1.0, 2.0),
        TerminalPointerPhase::Move,
        TerminalPointerButton::Left,
        9,
    ));

    // Act / Assert
    assert_eq!(left, right);
}

// ── TerminalKeyboardEvent ───────────────────────────────────────────────────

#[test]
fn should_retain_key_and_modifiers_when_key_down() {
    // Arrange
    let modifiers = TerminalModifiers {
        ctrl: true,
        ..TerminalModifiers::default()
    };

    // Act
    let event = TerminalKeyboardEvent::KeyDown {
        key: TerminalKey::Enter,
        modifiers,
    };

    // Assert
    match event {
        TerminalKeyboardEvent::KeyDown {
            key,
            modifiers: mods,
        } => {
            assert_eq!(key, TerminalKey::Enter);
            assert_eq!(mods, modifiers);
        }
        other => panic!("expected KeyDown, got {other:?}"),
    }
}

#[test]
fn should_retain_key_and_modifiers_when_key_up() {
    // Arrange
    let modifiers = TerminalModifiers {
        shift: true,
        ..TerminalModifiers::default()
    };

    // Act
    let event = TerminalKeyboardEvent::KeyUp {
        key: TerminalKey::Escape,
        modifiers,
    };

    // Assert
    match event {
        TerminalKeyboardEvent::KeyUp {
            key,
            modifiers: mods,
        } => {
            assert_eq!(key, TerminalKey::Escape);
            assert_eq!(mods, modifiers);
        }
        other => panic!("expected KeyUp, got {other:?}"),
    }
}

#[test]
fn should_distinguish_key_down_from_key_up_when_payloads_match() {
    // Arrange
    let modifiers = TerminalModifiers {
        ctrl: true,
        ..TerminalModifiers::default()
    };
    let down = TerminalKeyboardEvent::KeyDown {
        key: TerminalKey::Enter,
        modifiers,
    };
    let up = TerminalKeyboardEvent::KeyUp {
        key: TerminalKey::Enter,
        modifiers,
    };

    // Act / Assert
    assert_ne!(down, up);
}

#[test]
fn should_retain_empty_ime_text_when_constructed() {
    // Arrange / Act
    let event = TerminalKeyboardEvent::Ime(String::new());

    // Assert
    assert_eq!(event, TerminalKeyboardEvent::Ime(String::new()));
}

#[test]
fn should_retain_ascii_ime_text_when_constructed() {
    // Arrange / Act
    let event = TerminalKeyboardEvent::Ime("hello".into());

    // Assert
    assert_eq!(event, TerminalKeyboardEvent::Ime("hello".into()));
}

#[test]
fn should_retain_unicode_ime_text_when_constructed() {
    // Arrange / Act
    let event = TerminalKeyboardEvent::Ime("你好".into());

    // Assert
    assert_eq!(event, TerminalKeyboardEvent::Ime("你好".into()));
}

#[test]
fn should_compare_unequal_when_ime_text_differs() {
    // Arrange
    let empty = TerminalKeyboardEvent::Ime(String::new());
    let nonempty = TerminalKeyboardEvent::Ime("x".into());

    // Act / Assert
    assert_ne!(empty, nonempty);
}

// ── TerminalKey ─────────────────────────────────────────────────────────────

#[test]
fn should_expose_control_navigation_and_function_keys_including_f1_through_f12() {
    // Arrange
    let keys = [
        TerminalKey::Tab,
        TerminalKey::Enter,
        TerminalKey::Space,
        TerminalKey::Escape,
        TerminalKey::Backspace,
        TerminalKey::Insert,
        TerminalKey::Delete,
        TerminalKey::F1,
        TerminalKey::F2,
        TerminalKey::F3,
        TerminalKey::F4,
        TerminalKey::F5,
        TerminalKey::F6,
        TerminalKey::F7,
        TerminalKey::F8,
        TerminalKey::F9,
        TerminalKey::F10,
        TerminalKey::F11,
        TerminalKey::F12,
        TerminalKey::ArrowUp,
        TerminalKey::ArrowDown,
        TerminalKey::ArrowLeft,
        TerminalKey::ArrowRight,
        TerminalKey::Home,
        TerminalKey::End,
        TerminalKey::PageUp,
        TerminalKey::PageDown,
    ];

    // Act / Assert — each variant is constructible and equal to itself
    for (i, key) in keys.iter().enumerate() {
        assert_eq!(key, key);
        for (j, other) in keys.iter().enumerate() {
            if i != j {
                assert_ne!(key, other);
            }
        }
    }
}

#[test]
fn should_distinguish_character_nul_numpad_character_and_numpad_enter() {
    // Arrange
    let character = TerminalKey::Character('a');
    let nul = TerminalKey::Character('\0');
    let numpad_character = TerminalKey::NumpadCharacter('5');
    let numpad_enter = TerminalKey::NumpadEnter;

    // Act / Assert
    assert_eq!(character, TerminalKey::Character('a'));
    assert_eq!(nul, TerminalKey::Character('\0'));
    assert_eq!(numpad_character, TerminalKey::NumpadCharacter('5'));
    assert_eq!(numpad_enter, TerminalKey::NumpadEnter);
    assert_ne!(TerminalKey::Enter, numpad_enter);
    assert_ne!(
        TerminalKey::Character('a'),
        TerminalKey::NumpadCharacter('a')
    );
    assert_ne!(character, nul);
}

// ── TerminalModifiers ───────────────────────────────────────────────────────

#[test]
fn should_clear_all_flags_when_default() {
    // Arrange / Act
    let none = TerminalModifiers::default();

    // Assert
    assert!(!none.shift);
    assert!(!none.ctrl);
    assert!(!none.alt);
    assert!(!none.meta);
}

#[test]
fn should_compare_unequal_when_only_shift_is_set() {
    // Arrange
    let none = TerminalModifiers::default();
    let shift = TerminalModifiers {
        shift: true,
        ..TerminalModifiers::default()
    };

    // Act / Assert
    assert_ne!(none, shift);
    assert!(shift.shift);
    assert!(!shift.ctrl);
    assert!(!shift.alt);
    assert!(!shift.meta);
}

#[test]
fn should_compare_unequal_when_only_ctrl_is_set() {
    // Arrange
    let none = TerminalModifiers::default();
    let ctrl = TerminalModifiers {
        ctrl: true,
        ..TerminalModifiers::default()
    };

    // Act / Assert
    assert_ne!(none, ctrl);
    assert!(!ctrl.shift);
    assert!(ctrl.ctrl);
    assert!(!ctrl.alt);
    assert!(!ctrl.meta);
}

#[test]
fn should_compare_unequal_when_only_alt_is_set() {
    // Arrange
    let none = TerminalModifiers::default();
    let alt = TerminalModifiers {
        alt: true,
        ..TerminalModifiers::default()
    };

    // Act / Assert
    assert_ne!(none, alt);
    assert!(!alt.shift);
    assert!(!alt.ctrl);
    assert!(alt.alt);
    assert!(!alt.meta);
}

#[test]
fn should_compare_unequal_when_only_meta_is_set() {
    // Arrange
    let none = TerminalModifiers::default();
    let meta = TerminalModifiers {
        meta: true,
        ..TerminalModifiers::default()
    };

    // Act / Assert
    assert_ne!(none, meta);
    assert!(!meta.shift);
    assert!(!meta.ctrl);
    assert!(!meta.alt);
    assert!(meta.meta);
}

#[test]
fn should_compare_equal_when_all_flags_combined() {
    // Arrange
    let all = TerminalModifiers {
        shift: true,
        ctrl: true,
        alt: true,
        meta: true,
    };
    let same = TerminalModifiers {
        shift: true,
        ctrl: true,
        alt: true,
        meta: true,
    };

    // Act / Assert
    assert_eq!(all, same);
}

#[test]
fn should_treat_individual_flags_as_independent() {
    // Arrange
    let shift = TerminalModifiers {
        shift: true,
        ..TerminalModifiers::default()
    };
    let ctrl = TerminalModifiers {
        ctrl: true,
        ..TerminalModifiers::default()
    };
    let alt = TerminalModifiers {
        alt: true,
        ..TerminalModifiers::default()
    };
    let meta = TerminalModifiers {
        meta: true,
        ..TerminalModifiers::default()
    };

    // Act / Assert
    assert_ne!(shift, ctrl);
    assert_ne!(ctrl, alt);
    assert_ne!(alt, meta);
    assert_ne!(shift, meta);
}

// ── TerminalPointerEvent ────────────────────────────────────────────────────

#[test]
fn should_preserve_all_fields_when_constructed() {
    // Arrange
    let position = (-1.5, 2.25);
    let phase = TerminalPointerPhase::Down;
    let button = TerminalPointerButton::Left;
    let pointer_id = 7u64;

    // Act
    let event = TerminalPointerEvent::new(position, phase, button, pointer_id);

    // Assert
    assert_eq!(event.position, position);
    assert_eq!(event.phase, phase);
    assert_eq!(event.button, button);
    assert_eq!(event.pointer_id, pointer_id);
}

#[test]
fn should_compare_unequal_when_phase_differs() {
    // Arrange
    let down = TerminalPointerEvent::new(
        (-1.5, 2.25),
        TerminalPointerPhase::Down,
        TerminalPointerButton::Left,
        7,
    );
    let moved = TerminalPointerEvent::new(
        (-1.5, 2.25),
        TerminalPointerPhase::Move,
        TerminalPointerButton::Left,
        7,
    );

    // Act / Assert
    assert_ne!(down, moved);
}

#[test]
fn should_preserve_wheel_phase_when_constructed() {
    // Arrange
    let phase = TerminalPointerPhase::WheelLine { dx: 1.0, dy: -2.0 };

    // Act
    let event = TerminalPointerEvent::new((10.0, 20.0), phase, TerminalPointerButton::Middle, 3);

    // Assert
    assert_eq!(event.phase, phase);
    assert_eq!(event.button, TerminalPointerButton::Middle);
    assert_eq!(event.pointer_id, 3);
}

// ── TerminalPointerPhase ────────────────────────────────────────────────────

#[test]
fn should_distinguish_lifecycle_phases() {
    // Arrange
    let phases = [
        TerminalPointerPhase::Down,
        TerminalPointerPhase::Move,
        TerminalPointerPhase::Up,
        TerminalPointerPhase::Cancel,
    ];

    // Act / Assert
    for (i, phase) in phases.iter().enumerate() {
        assert_eq!(phase, phase);
        for (j, other) in phases.iter().enumerate() {
            if i != j {
                assert_ne!(phase, other);
            }
        }
    }
}

#[test]
fn should_distinguish_line_wheel_from_pixel_wheel_when_deltas_match() {
    // Arrange
    let line = TerminalPointerPhase::WheelLine { dx: 0.0, dy: -3.0 };
    let pixel = TerminalPointerPhase::WheelPixel { dx: 0.0, dy: -3.0 };

    // Act / Assert
    assert_ne!(line, pixel);
}

#[test]
fn should_preserve_zero_line_wheel_deltas() {
    // Arrange / Act
    let phase = TerminalPointerPhase::WheelLine { dx: 0.0, dy: 0.0 };

    // Assert
    assert_eq!(phase, TerminalPointerPhase::WheelLine { dx: 0.0, dy: 0.0 });
}

#[test]
fn should_preserve_positive_and_negative_line_wheel_deltas() {
    // Arrange / Act
    let positive = TerminalPointerPhase::WheelLine { dx: 2.0, dy: 1.0 };
    let negative = TerminalPointerPhase::WheelLine { dx: -2.0, dy: -1.0 };

    // Assert
    assert_eq!(
        positive,
        TerminalPointerPhase::WheelLine { dx: 2.0, dy: 1.0 }
    );
    assert_eq!(
        negative,
        TerminalPointerPhase::WheelLine { dx: -2.0, dy: -1.0 }
    );
    assert_ne!(positive, negative);
}

#[test]
fn should_preserve_zero_pixel_wheel_deltas() {
    // Arrange / Act
    let phase = TerminalPointerPhase::WheelPixel { dx: 0.0, dy: 0.0 };

    // Assert
    assert_eq!(phase, TerminalPointerPhase::WheelPixel { dx: 0.0, dy: 0.0 });
}

#[test]
fn should_preserve_positive_and_negative_pixel_wheel_deltas() {
    // Arrange / Act
    let positive = TerminalPointerPhase::WheelPixel { dx: 1.5, dy: 40.0 };
    let negative = TerminalPointerPhase::WheelPixel {
        dx: -1.5,
        dy: -40.0,
    };

    // Assert
    assert_eq!(
        positive,
        TerminalPointerPhase::WheelPixel { dx: 1.5, dy: 40.0 }
    );
    assert_eq!(
        negative,
        TerminalPointerPhase::WheelPixel {
            dx: -1.5,
            dy: -40.0
        }
    );
    assert_ne!(positive, negative);
}

// ── TerminalPointerButton ───────────────────────────────────────────────────

#[test]
fn should_compare_left_right_and_middle_independently() {
    // Arrange
    let left = TerminalPointerButton::Left;
    let right = TerminalPointerButton::Right;
    let middle = TerminalPointerButton::Middle;

    // Act / Assert
    assert_eq!(left, TerminalPointerButton::Left);
    assert_eq!(right, TerminalPointerButton::Right);
    assert_eq!(middle, TerminalPointerButton::Middle);
    assert_ne!(left, right);
    assert_ne!(right, middle);
    assert_ne!(left, middle);
}

// ── TerminalFocusEvent ──────────────────────────────────────────────────────

#[test]
fn should_represent_gained_and_lost_as_distinct_values() {
    // Arrange
    let gained = TerminalFocusEvent::Gained;
    let lost = TerminalFocusEvent::Lost;

    // Act / Assert
    assert_eq!(gained, TerminalFocusEvent::Gained);
    assert_eq!(lost, TerminalFocusEvent::Lost);
    assert_ne!(gained, lost);
}

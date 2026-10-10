//! Spec 0019: GPU-independent indexed palette, framing, and coherent update contracts.
use super::*;
use harbor_config::{Color, Palette, Rgba};
use std::time::Instant;

fn palette(terminal: &Terminal) -> Palette {
    terminal.read_update(Instant::now()).appearance.palette()
}

fn configured() -> Palette {
    let mut startup = Palette::default();
    startup.normal[1] = Rgba::from_rgba8(1, 2, 3, 64);
    startup.bright[7] = Rgba::from_rgba8(4, 5, 6, 128);
    startup
}

fn terminal(startup: Palette) -> Terminal {
    Terminal::new_headless_with_appearance(2, 20, TerminalAppearance::from_palette(startup))
}

#[test]
fn osc_palette_all_slots_baseline_aliases_alpha_and_reset() {
    let startup = configured();
    let levels = [0, 95, 135, 175, 215, 255];
    for index in 16..=231 {
        let n = (index - 16) as usize;
        assert_eq!(
            startup.indexed(index),
            Rgba::from_rgb8(levels[n / 36], levels[n / 6 % 6], levels[n % 6])
        );
    }
    for index in 232..=255 {
        let gray = 8 + (index - 232) * 10;
        assert_eq!(startup.indexed(index), Rgba::from_rgb8(gray, gray, gray));
    }
    let mut t = terminal(startup);
    for index in 0..=255 {
        t.put_str(&format!("\x1b]4;{index};#123456;{index};?\x1b\\"));
        assert_eq!(
            t.screen.drain_replies(),
            format!("\x1b]4;{index};rgb:1212/3434/5656\x1b\\").as_bytes()
        );
        let active = palette(&t);
        let alpha = startup.indexed(index).components()[3];
        assert_eq!(
            active.indexed(index),
            Rgba::new(
                0x12 as f32 / 255.0,
                0x34 as f32 / 255.0,
                0x56 as f32 / 255.0,
                alpha
            )
        );
        if index < 8 {
            assert_eq!(
                active.resolve(Color::Named(index)),
                active.resolve(Color::Indexed(index))
            );
        } else if index < 16 {
            assert_eq!(
                active.resolve(Color::Bright(index - 8)),
                active.resolve(Color::Indexed(index))
            );
        }
    }
    t.put_bytes(b"\x1b]104\x07");
    assert_eq!(palette(&t), startup);
    assert!(t.screen.drain_replies().is_empty());
}

#[test]
fn osc_palette_ordered_queries_exact_terminators_and_quantization() {
    let mut t = terminal(configured());
    t.put_bytes(b"\x1b]4;42;?;42;#112233;43;#abcdef;42;?;42;#445566;42;?\x1b\\");
    assert_eq!(t.screen.drain_replies(), b"\x1b]4;42;rgb:0000/d7d7/8787\x1b\\\x1b]4;42;rgb:1111/2222/3333\x1b\\\x1b]4;42;rgb:4444/5555/6666\x1b\\");
    assert_eq!(palette(&t).indexed(43), Rgba::from_rgb8(0xab, 0xcd, 0xef));
    t.put_bytes(b"\x1b]4;255;rgb:f/80/8000;255;?;1;#12AbEF;1;?\x07");
    assert_eq!(
        t.screen.drain_replies(),
        b"\x1b]4;255;rgb:ffff/8080/8080\x07\x1b]4;1;rgb:1212/abab/efef\x07"
    );
    assert_eq!(
        palette(&t).indexed(1),
        Rgba::from_rgba8(0x12, 0xab, 0xef, 64)
    );
    t.put_bytes(b"\x1b]4;255;rgb:8/800/FFFF;255;?\x1b\\");
    assert_eq!(
        t.screen.drain_replies(),
        b"\x1b]4;255;rgb:8888/8080/ffff\x1b\\"
    );
}

#[test]
fn osc_palette_rejects_entire_malformed_request_without_damage_or_replies() {
    let mut t = terminal(configured());
    let startup = palette(&t);
    let initial = t.read_update(Instant::now());
    assert!(t.acknowledge_update(&initial));
    for tail in [
        "",
        "42",
        "42;",
        ";?",
        "256;#abcdef",
        "-1;?",
        "+1;?",
        " 1;?",
        "429496729600;?",
        "42;red",
        "42;red?",
        "42;??",
        "42;?x",
        "42;rgbi:1/0/0",
        "42;#12345678",
        "42;#12345g",
        "42;rgb:/0/0",
        "42;rgb:0/0",
        "42;rgb:0/0/0/0",
        "42;rgb:00000/0/0",
        "42;rgb:0/0/g",
    ] {
        t.put_str(&format!("\x1b]4;42;#112233;42;?;{tail}\x07"));
        assert_eq!(palette(&t), startup, "{tail}");
        assert!(t.screen.drain_replies().is_empty(), "{tail}");
        assert!(t.snapshot().dirty_ranges.is_empty(), "{tail}");
    }
    for payload in ["", "?", "1", ";1", "1;"] {
        t.put_str(&format!("\x1b]4;{payload}\x07"));
        assert_eq!(palette(&t), startup);
        assert!(t.screen.drain_replies().is_empty());
    }
    t.put_bytes(b"\x1b]4;42;#123456;42;?\x1b\\ok");
    assert_eq!(
        t.screen.drain_replies(),
        b"\x1b]4;42;rgb:1212/3434/5656\x1b\\"
    );
    assert_eq!(t.snapshot().cell(0, 0).ch, 'o');
}

#[test]
fn osc_palette_selective_repeated_full_reset_and_default_separation() {
    let startup = configured();
    let mut t = terminal(startup);
    t.put_bytes(b"\x1b]4;1;#112233;42;#445566;255;#abcdef\x07\x1b]10;#102030\x07\x1b]11;#405060\x07\x1b]12;#708090\x07");
    let changed = palette(&t);
    for payload in [
        "1;256",
        "1;-1",
        "1;999999999999999999999",
        "1;",
        "1;;42",
        ";",
        "?",
        "1;#112233",
    ] {
        t.put_str(&format!("\x1b]104;{payload}\x07"));
        assert_eq!(palette(&t), changed, "{payload}");
    }
    t.put_bytes(b"\x1b]104;1;42;42\x1b\\");
    assert_eq!(palette(&t).indexed(1), startup.indexed(1));
    assert_eq!(palette(&t).indexed(42), startup.indexed(42));
    assert_eq!(palette(&t).indexed(255), changed.indexed(255));
    t.put_bytes(b"\x1b]104\x07");
    let reset = palette(&t);
    for index in 0..=255 {
        assert_eq!(reset.indexed(index), startup.indexed(index));
    }
    assert_eq!(reset.foreground, changed.foreground);
    assert_eq!(reset.background, changed.background);
    assert_eq!(reset.cursor, changed.cursor);
    assert_eq!(reset.selection, startup.selection);
    t.put_bytes(b"\x1b]4;42;#123456\x07\x1b]110\x07\x1b]111\x07\x1b]112\x07");
    assert_eq!(palette(&t).indexed(42), Rgba::from_rgb8(0x12, 0x34, 0x56));
    assert_eq!(palette(&t).foreground, startup.foreground);
    assert_eq!(palette(&t).background, startup.background);
    assert_eq!(palette(&t).cursor, startup.cursor);
    assert!(t.screen.drain_replies().is_empty());
}

#[test]
fn osc_palette_lifetime_across_buffers_resets_and_independent_sessions() {
    for mode in [47, 1047, 1049] {
        let startup = configured();
        let mut t = terminal(startup);
        let mut other_startup = startup;
        other_startup.normal[1] = Rgba::from_rgba8(7, 8, 9, 96);
        let mut other = terminal(other_startup);
        t.put_bytes(b"\x1b]4;1;#112233\x07");
        t.put_str(&format!(
            "\x1b[?{mode}h\x1b]4;42;#445566\x07\x1b[?{mode}l\x1b[?{mode}h"
        ));
        t.put_bytes(b"\x1b]4;1;?;42;?\x07");
        assert_eq!(
            t.screen.drain_replies(),
            b"\x1b]4;1;rgb:1111/2222/3333\x07\x1b]4;42;rgb:4444/5555/6666\x07"
        );
        t.put_str(&format!("\x1b[?{mode}l\x1b[0m\x1b[!p\x1bc"));
        assert_eq!(palette(&t).indexed(42), Rgba::from_rgb8(0x44, 0x55, 0x66));
        assert_eq!(
            palette(&t).indexed(1),
            Rgba::from_rgba8(0x11, 0x22, 0x33, 64)
        );
        assert_eq!(palette(&other), other_startup);
        t.put_bytes(b"\x1b]104\x1b\\");
        assert_eq!(palette(&t), startup);
        other.put_bytes(b"\x1b]4;1;#abcdef\x07\x1b]104;1\x07");
        assert_eq!(palette(&other), other_startup);
    }
}

#[test]
fn osc_palette_fragmented_framing_cancellation_overflow_and_recovery() {
    for ending in [b"\x07".as_slice(), b"\x1b\\"] {
        let mut stream = b"\x1b]4;1;#123456;255;rgb:f/80/8000;255;?".to_vec();
        stream.extend_from_slice(ending);
        stream.extend_from_slice(b"\x1b]104;1;1");
        stream.extend_from_slice(ending);
        stream.extend_from_slice(b"text");
        let mut whole = terminal(configured());
        whole.put_bytes(&stream);
        let expected = palette(&whole);
        let replies = whole.screen.drain_replies();
        for split in 0..=stream.len() {
            let mut t = terminal(configured());
            t.put_bytes(&stream[..split]);
            t.put_bytes(&stream[split..]);
            assert_eq!(palette(&t), expected, "split {split}");
            assert_eq!(t.screen.drain_replies(), replies, "split {split}");
            assert_eq!(t.snapshot(), whole.snapshot());
        }
        let mut t = terminal(configured());
        for byte in stream {
            t.put_bytes(&[byte]);
        }
        assert_eq!(palette(&t), expected);
        assert_eq!(t.screen.drain_replies(), replies);
    }
    for cancel in [0x18, 0x1a] {
        let mut t = terminal(configured());
        let startup = palette(&t);
        t.put_bytes(b"\x1b]4;1;#112233;1;?");
        assert_eq!(palette(&t), startup);
        assert!(t.screen.drain_replies().is_empty());
        t.put_bytes(&[cancel]);
        assert_eq!(palette(&t), startup);
        t.put_bytes(b"\x1b]4;1;#abcdef\x07\x1b]104;1");
        let changed = palette(&t);
        t.put_bytes(&[cancel]);
        assert_eq!(palette(&t), changed);
    }
    for command in ["4;1;#112233;1;?;", "104;1;"] {
        for ending in ["\x07", "\x1b\\"] {
            let mut t = terminal(configured());
            t.put_bytes(b"\x1b]4;1;#abcdef\x07");
            let before = palette(&t);
            t.put_str(&format!("\x1b]{command}{}{ending}", "0;".repeat(2049)));
            assert_eq!(palette(&t), before);
            assert!(t.screen.drain_replies().is_empty());
            t.put_bytes(b"ok\x1b]4;255;#123456;255;?\x07");
            assert_eq!(t.snapshot().cell(0, 0).ch, 'o');
            assert_eq!(
                t.screen.drain_replies(),
                b"\x1b]4;255;rgb:1212/3434/5656\x07"
            );
        }
    }
}

#[test]
fn osc_palette_reply_capacity_is_atomic_and_does_not_suppress_sets() {
    let mut t = terminal(configured());
    let reply = b"\x1b]4;255;rgb:1212/3434/5656\x1b\\";
    assert_eq!(reply.len(), 28);
    for extra in [0, 1] {
        let prefix = vec![b'x'; 1024 - reply.len() + extra];
        t.screen.push_reply(&prefix);
        t.put_bytes(b"\x1b]4;255;#123456;255;?;42;#abcdef\x1b\\");
        let replies = t.screen.drain_replies();
        assert_eq!(&replies[..prefix.len()], &prefix);
        assert_eq!(
            &replies[prefix.len()..],
            if extra == 0 { reply.as_slice() } else { b"" }
        );
        assert_eq!(palette(&t).indexed(42), Rgba::from_rgb8(0xab, 0xcd, 0xef));
    }
    t.put_str(&format!("\x1b]4;{}42;#112233\x1b\\", "255;?;".repeat(100)));
    let replies = t.screen.drain_replies();
    assert_eq!(replies, reply.repeat(1024 / reply.len()));
    assert_eq!(palette(&t).indexed(42), Rgba::from_rgb8(0x11, 0x22, 0x33));
}

#[test]
fn osc_palette_semantic_cells_skipped_updates_and_stale_acknowledgement() {
    let mut t = terminal(configured());
    for index in [0, 15, 16, 231, 232, 255] {
        t.put_str(&format!(
            "\x1b[1;4;38;5;{index};48;5;{index};58;5;{index}mX"
        ));
    }
    t.put_bytes(b"\x1b[0;31mA\x1b[97mB\x1b[38;2;10;20;30mT");
    let before = t.read_update(Instant::now());
    assert!(t.acknowledge_update(&before));
    let old = t.read_update(Instant::now());
    t.put_bytes(
        b"\x1b]4;0;#123456;15;#123456;16;#123456;231;#123456;232;#123456;255;#123456;1;#123456\x07",
    );
    assert!(!t.acknowledge_update(&old));
    let now = Instant::now();
    let updated = t.read_update(now);
    assert_eq!(updated.snapshot.cells, before.snapshot.cells);
    assert!(!updated.snapshot.dirty_ranges.is_empty());
    assert_eq!(updated.appearance.palette(), palette(&t));
    assert_eq!(
        t.read_update(now).snapshot.dirty_ranges,
        updated.snapshot.dirty_ranges
    );
    let active = updated.appearance.palette();
    for cell in &updated.snapshot.cells[..8] {
        assert_eq!(
            active.resolve(cell.fg)[..3],
            Rgba::from_rgb8(0x12, 0x34, 0x56).components()[..3]
        );
    }
    assert_eq!(
        active.resolve(updated.snapshot.cell(0, 8).fg),
        Rgba::from_rgb8(10, 20, 30).components()
    );
    assert!(t.acknowledge_update(&updated));
    assert!(t.snapshot().dirty_ranges.is_empty());
    t.put_bytes(b"\x1b]4;255;#123456\x07");
    assert!(t.snapshot().dirty_ranges.is_empty(), "no-op set");
    t.put_bytes(b"\x1b[?2026h\x1b]4;255;#abcdef\x07");
    let hidden = t.read_update(now);
    assert!(!hidden.frame_demand.ordinary_present_eligible);
    t.put_bytes(b"\x1b[?2026l");
    let replay = t.read_update(now);
    assert_eq!(
        replay.appearance.palette().indexed(255),
        Rgba::from_rgb8(0xab, 0xcd, 0xef)
    );
    assert!(!replay.snapshot.dirty_ranges.is_empty());
}

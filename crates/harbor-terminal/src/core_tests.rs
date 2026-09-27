use crate::{Preedit, Terminal, TerminalEvent, UpdateDamage};
use harbor_config::BLINK_INTERVAL_MS;
use std::time::{Duration, Instant};

#[test]
fn core_update_replays_skipped_output_and_requires_explicit_acknowledgement() {
    let mut engine = Terminal::new_headless(3, 8);
    let now = Instant::now();
    let initial = engine.read_update(now);
    assert_eq!(initial.damage, UpdateDamage::FullUpload);
    assert_eq!(initial.snapshot.rows, 3);
    assert!(engine.acknowledge_update(&initial));

    engine.put_bytes(b"ab");
    let hidden = engine.read_update(now);
    assert!(matches!(hidden.damage, UpdateDamage::Ranges(ref ranges) if !ranges.is_empty()));
    engine.put_bytes(b"c");
    assert!(!engine.acknowledge_update(&hidden));
    let replay = engine.read_update(now);
    assert_eq!(replay.snapshot.cell_char(0, 2), 'c');
    assert!(matches!(replay.damage, UpdateDamage::Ranges(ref ranges) if !ranges.is_empty()));
    assert_eq!(replay.snapshot, engine.read_update(now).snapshot);
    assert!(engine.acknowledge_update(&replay));
    assert_eq!(engine.read_update(now).damage, UpdateDamage::Ranges(vec![]));
    let before_failure = engine.read_update(now);
    engine.invalidate_update();
    assert_eq!(engine.read_update(now).damage, UpdateDamage::FullUpload);
    assert!(!engine.acknowledge_update(&before_failure));
    assert_eq!(engine.read_update(now).damage, UpdateDamage::FullUpload);
}

#[test]
fn core_blink_and_preedit_work_without_gpu() {
    let mut engine = Terminal::new_headless(3, 8);
    let now = Instant::now();
    let demand = engine.frame_demand(now);
    assert!(!demand.redraw_now);
    assert!(demand.deadline.is_some());
    engine.put_bytes(b"\x1b[2;2H");
    let update = engine.read_update(Instant::now());
    assert!(update.frame_demand.redraw_now);
    assert!(update.frame_demand.deadline.is_some());
    assert!(engine.acknowledge_update(&update));
    assert!(!engine.read_update(Instant::now()).frame_demand.redraw_now);

    engine
        .handle_event(TerminalEvent::Preedit(Preedit::new("compose", None)))
        .unwrap();
    assert!(engine.read_update(Instant::now()).frame_demand.redraw_now);
    assert_eq!(
        engine
            .read_update(Instant::now())
            .preedit
            .as_ref()
            .unwrap()
            .text,
        "compose"
    );
    engine.clear_preedit();
    assert!(engine.read_update(Instant::now()).frame_demand.redraw_now);
    engine.put_bytes(b"\x1b[2 q");
    assert_eq!(
        engine
            .frame_demand(now + Duration::from_millis(BLINK_INTERVAL_MS))
            .deadline,
        None
    );
}

#[test]
fn synchronized_output_eligibility_is_readable_without_consumption() {
    let mut engine = Terminal::new_headless(2, 10);
    engine.process_output(b"\x1b[?2026hhello");
    assert!(
        !engine
            .read_update(Instant::now())
            .frame_demand
            .ordinary_present_eligible
    );
    engine.process_output(b"\x1b[?2026l");
    assert!(
        engine
            .read_update(Instant::now())
            .frame_demand
            .ordinary_present_eligible
    );
    assert!(engine.read_update(Instant::now()).frame_demand.redraw_now);
}

#[test]
fn update_includes_selection_and_effective_appearance() {
    use crate::{
        RenderViewport, TerminalPointerButton, TerminalPointerEvent, TerminalPointerPhase,
    };
    let mut engine = Terminal::new_headless(2, 10);
    engine
        .pointer
        .set_viewport(RenderViewport::with_padding(10.0, 20.0, 0.0));
    engine.put_str("abcdefgh");
    for (phase, x) in [
        (TerminalPointerPhase::Down, 21.0),
        (TerminalPointerPhase::Move, 41.0),
        (TerminalPointerPhase::Up, 41.0),
    ] {
        engine
            .handle_event(TerminalEvent::Pointer(TerminalPointerEvent::new(
                (x, 1.0),
                phase,
                TerminalPointerButton::Left,
                1,
            )))
            .unwrap();
    }
    let update = engine.read_update(Instant::now());
    assert_eq!(update.selection, engine.pointer.bounds());
    assert!(update.selection.is_some());
    assert_eq!(update.appearance.palette(), engine.screen.active_palette());
    engine.set_backdrop_available(true);
    assert!(engine.read_update(Instant::now()).backdrop_available);
    assert!(!engine.acknowledge_update(&update));
}

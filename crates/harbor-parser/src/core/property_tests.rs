use proptest::prelude::*;
use proptest::test_runner::{Config, RngSeed};

use super::Parser;
use crate::params::Params;
use crate::perform::VtHandler;

#[derive(Clone, Debug, Eq, PartialEq)]
enum CallbackEvent {
    Print(char),
    Execute(u8),
    Csi {
        params: Params,
        intermediates: Vec<u8>,
        action: u8,
        private_marker: Option<u8>,
    },
    Esc {
        intermediates: Vec<u8>,
        byte: u8,
    },
    Osc {
        params: Vec<Vec<u8>>,
        bell_terminated: bool,
    },
    DcsHook {
        params: Params,
        intermediates: Vec<u8>,
        action: u8,
    },
    DcsPut(u8),
    DcsUnhook(bool),
    StartString(u8),
}

#[derive(Default)]
struct RecordingHandler {
    events: Vec<CallbackEvent>,
    current_string_bytes: usize,
    max_string_bytes: usize,
}

impl VtHandler for RecordingHandler {
    fn print(&mut self, ch: char) {
        self.events.push(CallbackEvent::Print(ch));
    }

    fn execute(&mut self, byte: u8) {
        self.events.push(CallbackEvent::Execute(byte));
    }

    fn csi_dispatch(
        &mut self,
        params: &Params,
        intermediates: &[u8],
        action: u8,
        private_marker: Option<u8>,
    ) {
        self.events.push(CallbackEvent::Csi {
            params: *params,
            intermediates: intermediates.to_vec(),
            action,
            private_marker,
        });
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], byte: u8) {
        self.events.push(CallbackEvent::Esc {
            intermediates: intermediates.to_vec(),
            byte,
        });
    }

    fn osc_dispatch(&mut self, params: &[&[u8]], bell_terminated: bool) {
        self.events.push(CallbackEvent::Osc {
            params: params.iter().map(|param| param.to_vec()).collect(),
            bell_terminated,
        });
    }

    fn dcs_hook(&mut self, params: &Params, intermediates: &[u8], action: u8) {
        self.current_string_bytes = 0;
        self.events.push(CallbackEvent::DcsHook {
            params: *params,
            intermediates: intermediates.to_vec(),
            action,
        });
    }

    fn dcs_put(&mut self, byte: u8) {
        self.current_string_bytes += 1;
        self.max_string_bytes = self.max_string_bytes.max(self.current_string_bytes);
        self.events.push(CallbackEvent::DcsPut(byte));
    }

    fn dcs_unhook(&mut self, terminated: bool) {
        self.current_string_bytes = 0;
        self.events.push(CallbackEvent::DcsUnhook(terminated));
    }

    fn start_string(&mut self, kind: u8) {
        self.current_string_bytes = 0;
        self.events.push(CallbackEvent::StartString(kind));
    }
}

fn feed_contiguous(input: &[u8], c1_enabled: bool) -> RecordingHandler {
    let mut parser = Parser::default();
    parser.set_c1_enabled(c1_enabled);
    let mut handler = RecordingHandler::default();
    let mut completed_calls = 0;

    for &byte in input {
        parser.advance(&mut handler, byte);
        completed_calls += 1;
        assert!(parser.retained_state_within_limits());
    }

    assert_eq!(completed_calls, input.len());
    assert!(handler.max_string_bytes <= 4096);
    handler
}

fn parser_input() -> impl Strategy<Value = Vec<u8>> {
    prop::collection::vec(any::<u8>(), 0..=16 * 1024)
}

fn feed_scheduled(input: &[u8], schedule: &[usize], c1_enabled: bool) -> RecordingHandler {
    let mut parser = Parser::default();
    parser.set_c1_enabled(c1_enabled);
    let mut handler = RecordingHandler::default();
    let mut offset = 0;
    let mut schedule_index = 0;
    let mut completed_calls = 0;

    while offset < input.len() {
        let requested = schedule.get(schedule_index).copied().unwrap_or(1).max(1);
        let end = offset.saturating_add(requested).min(input.len());
        for &byte in &input[offset..end] {
            parser.advance(&mut handler, byte);
            completed_calls += 1;
            assert!(parser.retained_state_within_limits());
        }
        offset = end;
        schedule_index += 1;
    }

    assert_eq!(completed_calls, input.len());
    assert!(handler.max_string_bytes <= 4096);
    handler
}

proptest! {
    #![proptest_config(Config {
        cases: 256,
        rng_seed: RngSeed::Fixed(0x0072_0220_0000_0072),
        ..Config::default()
    })]

    #[test]
    fn arbitrary_input_is_progressing_and_retained(input in parser_input()) {
        for c1_enabled in [false, true] {
            let result = feed_contiguous(&input, c1_enabled);
            prop_assert!(result.max_string_bytes <= 4096);
        }
    }

    #[test]
    fn scheduled_input_has_same_callback_trace_as_contiguous_input(
        input in parser_input(),
        schedule in prop::collection::vec(1usize..=64, 0..=512),
    ) {
        for c1_enabled in [false, true] {
            let contiguous = feed_contiguous(&input, c1_enabled);
            let scheduled = feed_scheduled(&input, &schedule, c1_enabled);
            prop_assert_eq!(scheduled.events, contiguous.events);
        }
    }

    #[test]
    fn over_limit_strings_recover(
        family in 0u8..5,
        cancellation in 0u8..4,
        payload in prop::collection::vec(0x20u8..=0x7e, 4097..=4100),
    ) {
        let mut input = match family {
            0 => b"\x1b]".to_vec(),
            1 => b"\x1bPq".to_vec(),
            2 => b"\x1b_".to_vec(),
            3 => b"\x1b^".to_vec(),
            _ => b"\x1bX".to_vec(),
        };
        input.extend_from_slice(&payload);
        match (family == 0, cancellation) {
            (true, 0) => input.push(0x07),
            (true, _) => input.extend_from_slice(b"\x1b\\"),
            (false, 0) => input.extend_from_slice(b"\x1b\\"),
            (false, 1) => input.push(0x18),
            (false, _) => input.push(0x1a),
        }
        input.push(b'Z');

        for c1_enabled in [false, true] {
            let result = feed_contiguous(&input, c1_enabled);
            prop_assert!(result.max_string_bytes <= 4096);
            prop_assert_eq!(result.events.last(), Some(&CallbackEvent::Print('Z')));
        }
    }
}

#[test]
fn should_recognize_eight_bit_sequences_only_when_c1_is_enabled() {
    // Arrange — include 8-bit CSI, OSC, DCS, and APC introductions and ST terminators.
    let input = [
        0x9b, b'1', b'm', 0x9d, b'0', b';', b'x', 0x9c, 0x90, b'q', b'a', 0x9c, 0x9f, b'b', 0x9c,
        b'Z',
    ];

    // Act — parse through the byte-at-a-time core in each C1 mode.
    let enabled = feed_contiguous(&input, true);
    let disabled = feed_contiguous(&input, false);

    // Assert — mode-local traces remain exact, and only enabled mode dispatches C1 sequences.
    assert!(
        enabled
            .events
            .iter()
            .any(|event| matches!(event, CallbackEvent::Csi { action: b'm', .. }))
    );
    assert!(enabled.events.iter().any(|event| matches!(
        event,
        CallbackEvent::Osc {
            bell_terminated: false,
            ..
        }
    )));
    assert!(
        enabled
            .events
            .iter()
            .any(|event| matches!(event, CallbackEvent::DcsHook { action: b'q', .. }))
    );
    assert!(
        enabled
            .events
            .iter()
            .any(|event| matches!(event, CallbackEvent::StartString(b'_')))
    );
    assert!(
        disabled
            .events
            .iter()
            .all(|event| !matches!(event, CallbackEvent::Csi { .. } | CallbackEvent::Osc { .. }))
    );
    assert!(disabled.events.iter().all(|event| !matches!(
        event,
        CallbackEvent::DcsHook { .. } | CallbackEvent::StartString(_)
    )));
    assert!(enabled.events.last() == Some(&CallbackEvent::Print('Z')));
}

#[test]
fn should_bound_and_recover_each_string_family_at_limit_and_overflow() {
    // Arrange — use every string family, both payload boundaries, and valid endings.
    for c1_enabled in [false, true] {
        for family in 0u8..5 {
            let endings: &[u8] = if family == 0 {
                if c1_enabled {
                    &[0, 1, 2, 3, 4]
                } else {
                    &[0, 1, 3, 4]
                }
            } else if c1_enabled {
                &[1, 2, 3, 4]
            } else {
                &[1, 3, 4]
            };

            for &payload_len in &[4096usize, 4097] {
                for &ending in endings {
                    let mut input = match family {
                        0 => b"\x1b]".to_vec(),
                        1 => b"\x1bPq".to_vec(),
                        2 => b"\x1b_".to_vec(),
                        3 => b"\x1b^".to_vec(),
                        _ => b"\x1bX".to_vec(),
                    };
                    input.extend(std::iter::repeat_n(b'a', payload_len));
                    match ending {
                        0 => input.push(0x07),
                        1 => input.extend_from_slice(b"\x1b\\"),
                        2 => input.push(0x9c),
                        3 => input.push(0x18),
                        4 => input.push(0x1a),
                        _ => unreachable!(),
                    }
                    input.push(b'Z');

                    // Act — parse continuously while crossing the payload limit.
                    let contiguous = feed_contiguous(&input, c1_enabled);

                    // Assert — the parser remains bounded and recovers after termination.
                    assert_eq!(contiguous.events.last(), Some(&CallbackEvent::Print('Z')));
                    if family == 0 {
                        // OSC retains its payload internally and reports only on termination.
                        let dispatched = contiguous
                            .events
                            .iter()
                            .any(|event| matches!(event, CallbackEvent::Osc { .. }));
                        let completed = matches!(ending, 0..=2);
                        assert_eq!(dispatched, payload_len == 4096 && completed);
                    } else {
                        let puts = contiguous
                            .events
                            .iter()
                            .filter(|event| matches!(event, CallbackEvent::DcsPut(_)))
                            .count();
                        assert_eq!(puts, payload_len.min(4096));
                        let unhooked = contiguous
                            .events
                            .iter()
                            .filter(|event| matches!(event, CallbackEvent::DcsUnhook(_)))
                            .count();
                        assert_eq!(unhooked, 1);
                        assert!(contiguous.events.iter().any(|event| matches!(
                            event,
                            CallbackEvent::DcsUnhook(terminated) if *terminated == (ending == 1 || ending == 2)
                        )));
                    }
                }
            }
        }
    }
}

#[test]
fn retained_state_handles_representative_protocol_boundaries() {
    let mut inputs = vec![
        b"\x1b[?1;2:3m".to_vec(),
        b"\x1b[123456789012345678901234567890m".to_vec(),
        b"\x1b[1\x1b[2m".to_vec(),
        b"\x1b]0;title\x07Z".to_vec(),
        b"\x1bP$qpayload\x18Z".to_vec(),
        b"\x1b_X\x1b\\Z".to_vec(),
        vec![0xf0, 0x9f, 0x92, 0xa1, b'Z'],
    ];
    inputs.push([b"\x1b]", &[b'a'; 4096][..], b"\x1b\\Z"].concat());

    for input in inputs {
        let mut parser = Parser::default();
        let mut handler = RecordingHandler::default();
        for (index, byte) in input.into_iter().enumerate() {
            parser.advance(&mut handler, byte);
            assert!(
                parser.retained_state_within_limits(),
                "retained state exceeded at index {index}, byte {byte:#04x}"
            );
        }
    }
}

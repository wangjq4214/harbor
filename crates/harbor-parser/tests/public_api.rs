use harbor_parser::{Params, Parser, VtHandler};

#[derive(Debug, Eq, PartialEq)]
enum Callback {
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
    callbacks: Vec<Callback>,
}

impl VtHandler for RecordingHandler {
    fn print(&mut self, ch: char) {
        self.callbacks.push(Callback::Print(ch));
    }

    fn execute(&mut self, byte: u8) {
        self.callbacks.push(Callback::Execute(byte));
    }

    fn csi_dispatch(
        &mut self,
        params: &Params,
        intermediates: &[u8],
        action: u8,
        private_marker: Option<u8>,
    ) {
        self.callbacks.push(Callback::Csi {
            params: *params,
            intermediates: intermediates.to_vec(),
            action,
            private_marker,
        });
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], byte: u8) {
        self.callbacks.push(Callback::Esc {
            intermediates: intermediates.to_vec(),
            byte,
        });
    }

    fn osc_dispatch(&mut self, params: &[&[u8]], bell_terminated: bool) {
        self.callbacks.push(Callback::Osc {
            params: params.iter().map(|param| param.to_vec()).collect(),
            bell_terminated,
        });
    }

    fn dcs_hook(&mut self, params: &Params, intermediates: &[u8], action: u8) {
        self.callbacks.push(Callback::DcsHook {
            params: *params,
            intermediates: intermediates.to_vec(),
            action,
        });
    }

    fn dcs_put(&mut self, byte: u8) {
        self.callbacks.push(Callback::DcsPut(byte));
    }

    fn dcs_unhook(&mut self, terminated: bool) {
        self.callbacks.push(Callback::DcsUnhook(terminated));
    }

    fn start_string(&mut self, kind: u8) {
        self.callbacks.push(Callback::StartString(kind));
    }
}

fn feed(parser: &mut Parser, handler: &mut RecordingHandler, bytes: &[u8]) {
    for &byte in bytes {
        parser.advance(handler, byte);
    }
}

#[test]
fn should_expose_root_api_and_preserve_csi_params_when_private_csi_is_dispatched() {
    // Arrange — use only the public root exports and a recording VtHandler.
    let mut parser = Parser::default();
    let mut handler = RecordingHandler::default();

    // Act — parse a private CSI sequence with empty and colon-separated parameter slots.
    feed(&mut parser, &mut handler, b"\x1b[?1;:2:3m");

    // Assert — the handler sees the new public callback shape and Params accessors.
    let [
        Callback::Csi {
            params,
            intermediates,
            action,
            private_marker,
        },
    ] = handler.callbacks.as_slice()
    else {
        panic!("expected one CSI callback, got {:?}", handler.callbacks);
    };
    assert_eq!(*intermediates, []);
    assert_eq!(*action, b'm');
    assert_eq!(*private_marker, Some(b'?'));
    assert_eq!(params.len(), 2);
    assert_eq!(params.get(0), Some(1));
    assert_eq!(params.sub_params_len(1), Some(3));
    assert_eq!(params.get_sub_param(1, 0), None);
    assert_eq!(params.get_sub_param(1, 1), Some(2));
    assert_eq!(params.get_sub_param(1, 2), Some(3));
}

#[test]
fn should_preserve_each_csi_private_marker_when_prefix_is_present_or_absent() {
    // Arrange — cover the ordinary CSI form and every supported private prefix.
    let cases = [
        (b"\x1b[0m".as_slice(), None),
        (b"\x1b[?0m".as_slice(), Some(b'?')),
        (b"\x1b[>0m".as_slice(), Some(b'>')),
        (b"\x1b[<0m".as_slice(), Some(b'<')),
        (b"\x1b[=0m".as_slice(), Some(b'=')),
    ];

    // Act — parse each sequence through the public parser and recording handler.
    let observed_markers = cases
        .iter()
        .map(|(sequence, _)| {
            let mut parser = Parser::default();
            let mut handler = RecordingHandler::default();
            feed(&mut parser, &mut handler, sequence);

            let [Callback::Csi { private_marker, .. }] = handler.callbacks.as_slice() else {
                panic!("expected one CSI callback, got {:?}", handler.callbacks);
            };
            *private_marker
        })
        .collect::<Vec<_>>();

    // Assert — the callback preserves None, '?', '>', '<', and '=' exactly.
    let expected_markers = cases.iter().map(|(_, marker)| *marker).collect::<Vec<_>>();
    assert_eq!(observed_markers, expected_markers);
}

#[test]
fn should_suppress_csi_dispatch_when_private_marker_arrives_after_parameters() {
    // Arrange
    let mut parser = Parser::default();
    let mut handler = RecordingHandler::default();

    // Act
    feed(&mut parser, &mut handler, b"\x1b[0>c\x1b[5m");

    // Assert — the malformed CSI is consumed, while the following valid CSI dispatches.
    assert_eq!(
        handler.callbacks,
        vec![Callback::Csi {
            params: Params::from(&[Some(5)][..]),
            intermediates: Vec::new(),
            action: b'm',
            private_marker: None,
        }]
    );
}

#[test]
fn should_suppress_csi_dispatch_when_private_marker_is_repeated() {
    // Arrange
    let mut parser = Parser::default();
    let mut handler = RecordingHandler::default();

    // Act
    feed(&mut parser, &mut handler, b"\x1b[>>0c\x1b[5m");

    // Assert — a repeated marker cannot become a valid private prefix or leak state.
    assert_eq!(
        handler.callbacks,
        vec![Callback::Csi {
            params: Params::from(&[Some(5)][..]),
            intermediates: Vec::new(),
            action: b'm',
            private_marker: None,
        }]
    );
}

#[test]
fn should_consume_malformed_dcs_when_private_marker_is_late_or_repeated() {
    // Arrange
    let malformed_sequences = [
        b"\x1bP0>qpayload\x1b\\Z".as_slice(),
        b"\x1bP>>0qpayload\x1b\\Z".as_slice(),
    ];

    // Act
    let callbacks = malformed_sequences
        .iter()
        .map(|sequence| {
            let mut parser = Parser::default();
            let mut handler = RecordingHandler::default();
            feed(&mut parser, &mut handler, sequence);
            handler.callbacks
        })
        .collect::<Vec<_>>();

    // Assert — malformed DCS payloads are swallowed through ST, then text resumes.
    assert_eq!(
        callbacks,
        vec![vec![Callback::Print('Z')], vec![Callback::Print('Z')]]
    );
}

#[test]
fn should_return_none_from_params_accessors_when_slot_is_out_of_range() {
    // Arrange — construct a one-slot Params value through its public conversion API.
    let params = Params::from(&[Some(7)][..]);

    // Act — query the first out-of-range parameter slot.
    let sub_params_len = params.sub_params_len(1);

    // Assert — callers cannot observe unused fixed-capacity slots.
    assert_eq!(params.len(), 1);
    assert_eq!(params.get(1), None);
    assert_eq!(sub_params_len, None);
    assert_eq!(params.get_sub_param(1, 0), None);
}

#[test]
fn should_preserve_escape_intermediate_bytes() {
    let mut parser = Parser::default();
    let mut handler = RecordingHandler::default();

    feed(&mut parser, &mut handler, b"\x1b(B");

    assert_eq!(
        handler.callbacks,
        vec![Callback::Esc {
            intermediates: vec![b'('],
            byte: b'B',
        }]
    );
}

#[test]
fn should_dispatch_one_and_multiple_escape_intermediates() {
    let cases = [
        (b"\x1b#8".as_slice(), vec![b'#']),
        (b"\x1b##8".as_slice(), vec![b'#', b'#']),
    ];

    for (sequence, intermediates) in cases {
        let mut parser = Parser::default();
        let mut handler = RecordingHandler::default();

        feed(&mut parser, &mut handler, sequence);

        assert_eq!(
            handler.callbacks,
            vec![Callback::Esc {
                intermediates,
                byte: b'8',
            }]
        );
    }
}

#[test]
fn should_consume_cancelled_or_overflowed_escape_intermediates_and_resume_text() {
    for cancel in [0x18u8, 0x1au8] {
        let mut parser = Parser::default();
        let mut handler = RecordingHandler::default();
        let mut sequence = b"\x1b#".to_vec();
        sequence.push(cancel);
        sequence.extend_from_slice(b"Z");

        feed(&mut parser, &mut handler, &sequence);

        assert_eq!(handler.callbacks, vec![Callback::Print('Z')]);
    }

    let mut parser = Parser::default();
    let mut handler = RecordingHandler::default();
    feed(&mut parser, &mut handler, b"\x1b###8Z");
    assert_eq!(handler.callbacks, vec![Callback::Print('Z')]);
}

#[test]
fn should_emit_dcs_lifecycle_callbacks_when_payload_is_st_terminated() {
    // Arrange — create an isolated parser and recording handler.
    let mut parser = Parser::default();
    let mut handler = RecordingHandler::default();

    // Act — parse a DCS introducer, payload, and ST terminator.
    feed(&mut parser, &mut handler, b"\x1bP1;2qhi\x1b\\");

    // Assert — dcs_hook, dcs_put, and dcs_unhook occur in lifecycle order.
    assert_eq!(
        handler.callbacks,
        vec![
            Callback::DcsHook {
                params: Params::from(&[Some(1), Some(2)][..]),
                intermediates: vec![],
                action: b'q',
            },
            Callback::DcsPut(b'h'),
            Callback::DcsPut(b'i'),
            Callback::DcsUnhook(true),
        ]
    );
}

#[test]
fn should_emit_string_lifecycle_callbacks_when_apc_is_st_terminated() {
    // Arrange — create an isolated parser and recording handler.
    let mut parser = Parser::default();
    let mut handler = RecordingHandler::default();

    // Act — parse an APC string sequence and its ST terminator.
    feed(&mut parser, &mut handler, b"\x1b_ab\x1b\\");

    // Assert — string payload uses the public dcs_put and dcs_unhook callbacks.
    assert_eq!(
        handler.callbacks,
        vec![
            Callback::StartString(b'_'),
            Callback::DcsPut(b'a'),
            Callback::DcsPut(b'b'),
            Callback::DcsUnhook(true),
        ]
    );
}

#[test]
fn should_preserve_dcs_intermediate_bytes() {
    let mut parser = Parser::default();
    let mut handler = RecordingHandler::default();

    feed(&mut parser, &mut handler, b"\x1bP$qpayload\x1b\\Z");

    assert_eq!(
        handler.callbacks,
        vec![
            Callback::DcsHook {
                params: Params::from(&[None][..]),
                intermediates: vec![b'$'],
                action: b'q',
            },
            Callback::DcsPut(b'p'),
            Callback::DcsPut(b'a'),
            Callback::DcsPut(b'y'),
            Callback::DcsPut(b'l'),
            Callback::DcsPut(b'o'),
            Callback::DcsPut(b'a'),
            Callback::DcsPut(b'd'),
            Callback::DcsUnhook(true),
            Callback::Print('Z'),
        ]
    );
}

#[test]
fn should_report_dcs_cancellation_when_can_or_sub_terminates_payload() {
    for cancel in [0x18u8, 0x1au8] {
        let mut parser = Parser::default();
        let mut handler = RecordingHandler::default();
        let mut bytes = b"\x1bP$qm".to_vec();
        bytes.push(cancel);
        bytes.extend_from_slice(b"Z");
        feed(&mut parser, &mut handler, &bytes);

        assert_eq!(
            handler.callbacks,
            vec![
                Callback::DcsHook {
                    params: Params::from(&[None][..]),
                    intermediates: vec![b'$'],
                    action: b'q',
                },
                Callback::DcsPut(b'm'),
                Callback::DcsUnhook(false),
                Callback::Print('Z'),
            ]
        );
    }
}

#[test]
fn should_report_string_cancellation_when_can_or_sub_terminates_apc_pm_or_sos() {
    // Arrange / Act / Assert — string-family hooks end with terminated=false on CAN/SUB.
    for (introducer, kind) in [(b'_', b'_'), (b'^', b'^'), (b'X', b'X')] {
        for cancel in [0x18u8, 0x1au8] {
            let mut parser = Parser::default();
            let mut handler = RecordingHandler::default();
            let mut bytes = vec![0x1b, introducer, b'a'];
            bytes.push(cancel);
            bytes.extend_from_slice(b"Z");
            feed(&mut parser, &mut handler, &bytes);

            assert_eq!(
                handler.callbacks,
                vec![
                    Callback::StartString(kind),
                    Callback::DcsPut(b'a'),
                    Callback::DcsUnhook(false),
                    Callback::Print('Z'),
                ]
            );
        }
    }
}

#[test]
fn should_report_dcs_completion_when_eight_bit_st_terminates_payload() {
    // Arrange
    let mut parser = Parser::default();
    parser.set_c1_enabled(true);
    let mut handler = RecordingHandler::default();

    // Act — DCS payload ended by enabled 8-bit ST (0x9C).
    feed(&mut parser, &mut handler, b"\x1bP$qm\x9cZ");

    // Assert
    assert_eq!(
        handler.callbacks,
        vec![
            Callback::DcsHook {
                params: Params::from(&[None][..]),
                intermediates: vec![b'$'],
                action: b'q',
            },
            Callback::DcsPut(b'm'),
            Callback::DcsUnhook(true),
            Callback::Print('Z'),
        ]
    );
}

#[test]
fn should_report_dcs_cancellation_when_can_arrives_during_escape() {
    // Arrange
    let mut parser = Parser::default();
    let mut handler = RecordingHandler::default();

    // Act — ESC enters DCS escape; CAN cancels instead of completing ST.
    feed(&mut parser, &mut handler, b"\x1bP$qm\x1b\x18Z");

    // Assert
    assert_eq!(
        handler.callbacks,
        vec![
            Callback::DcsHook {
                params: Params::from(&[None][..]),
                intermediates: vec![b'$'],
                action: b'q',
            },
            Callback::DcsPut(b'm'),
            Callback::DcsUnhook(false),
            Callback::Print('Z'),
        ]
    );
}

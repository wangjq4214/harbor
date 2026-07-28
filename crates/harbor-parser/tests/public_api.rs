use harbor_parser::{Params, Parser, VtHandler};

#[derive(Debug, Eq, PartialEq)]
enum Callback {
    Print(char),
    Execute(u8),
    Csi {
        params: Params,
        intermediates: Vec<u8>,
        action: u8,
        private: bool,
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
    DcsUnhook,
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

    fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], action: u8, private: bool) {
        self.callbacks.push(Callback::Csi {
            params: *params,
            intermediates: intermediates.to_vec(),
            action,
            private,
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

    fn dcs_unhook(&mut self) {
        self.callbacks.push(Callback::DcsUnhook);
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
            private,
        },
    ] = handler.callbacks.as_slice()
    else {
        panic!("expected one CSI callback, got {:?}", handler.callbacks);
    };
    assert_eq!(*intermediates, []);
    assert_eq!(*action, b'm');
    assert!(*private);
    assert_eq!(params.len(), 2);
    assert_eq!(params.get(0), Some(1));
    assert_eq!(params.sub_params_len(1), Some(3));
    assert_eq!(params.get_sub_param(1, 0), None);
    assert_eq!(params.get_sub_param(1, 1), Some(2));
    assert_eq!(params.get_sub_param(1, 2), Some(3));
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
            Callback::DcsUnhook,
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
            Callback::DcsUnhook,
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
            Callback::DcsUnhook,
            Callback::Print('Z'),
        ]
    );
}

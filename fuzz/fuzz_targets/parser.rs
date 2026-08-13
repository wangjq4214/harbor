#![no_main]

use harbor_parser::{Params, Parser, VtHandler};
use harbor_parser_fuzz::{decode_schedule_payload, for_each_scheduled_chunk};
use libfuzzer_sys::fuzz_target;

#[derive(Default)]
struct NoopFuzzHandler;

impl VtHandler for NoopFuzzHandler {
    fn print(&mut self, _ch: char) {}

    fn execute(&mut self, _byte: u8) {}

    fn csi_dispatch(
        &mut self,
        _params: &Params,
        _intermediates: &[u8],
        _action: u8,
        _private_marker: Option<u8>,
    ) {
    }

    fn esc_dispatch(&mut self, _intermediates: &[u8], _byte: u8) {}

    fn osc_dispatch(&mut self, _params: &[&[u8]], _bell_terminated: bool) {}

    fn dcs_hook(&mut self, _params: &Params, _intermediates: &[u8], _action: u8) {}

    fn dcs_put(&mut self, _byte: u8) {}

    fn dcs_unhook(&mut self, _terminated: bool) {}

    fn start_string(&mut self, _kind: u8) {}
}

fn feed_contiguous(parser: &mut Parser, handler: &mut NoopFuzzHandler, payload: &[u8]) {
    for &byte in payload {
        parser.advance(handler, byte);
    }
}

fn feed_scheduled(
    parser: &mut Parser,
    handler: &mut NoopFuzzHandler,
    schedule: &[u8],
    payload: &[u8],
) {
    for_each_scheduled_chunk(schedule, payload, |chunk| {
        feed_contiguous(parser, handler, chunk);
    });
}

fuzz_target!(|data: &[u8]| {
    let (schedule, payload) = decode_schedule_payload(data);

    for c1_enabled in [false, true] {
        let mut parser = Parser::default();
        parser.set_c1_enabled(c1_enabled);
        let mut handler = NoopFuzzHandler;
        feed_contiguous(&mut parser, &mut handler, payload);

        let mut parser = Parser::default();
        parser.set_c1_enabled(c1_enabled);
        let mut handler = NoopFuzzHandler;
        feed_scheduled(&mut parser, &mut handler, schedule, payload);
    }
});

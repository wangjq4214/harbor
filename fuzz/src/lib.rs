/// Split fuzz input into a bounded schedule prefix and parser payload.
///
/// Inputs up to 32 bytes are entirely payload so every short input reaches the parser. For
/// longer inputs, the first 32 bytes retain the checked-in corpus schedule format and the rest
/// is parser payload.
pub fn decode_schedule_payload(data: &[u8]) -> (&[u8], &[u8]) {
    let schedule_len = if data.len() > 32 { 32 } else { 0 };
    data.split_at(schedule_len)
}

/// Visit payload chunks selected by the bounded schedule prefix.
///
/// A zero schedule byte and an exhausted schedule both select a one-byte chunk, so every
/// non-empty payload is consumed and an empty or short schedule cannot stall the harness.
pub fn for_each_scheduled_chunk<F>(schedule: &[u8], payload: &[u8], mut consume: F)
where
    F: FnMut(&[u8]),
{
    let mut offset = 0;
    let mut schedule_index = 0;
    while offset < payload.len() {
        let requested = schedule.get(schedule_index).copied().unwrap_or(1).max(1) as usize;
        let end = offset.saturating_add(requested).min(payload.len());
        consume(&payload[offset..end]);
        offset = end;
        schedule_index += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::{decode_schedule_payload, for_each_scheduled_chunk};
    use harbor_parser::{Params, Parser, VtHandler};

    #[derive(Clone, Debug, Eq, PartialEq)]
    enum Event {
        Print(char),
        Execute(u8),
        Csi(Params, Vec<u8>, u8, Option<u8>),
        Esc(Vec<u8>, u8),
        Osc(Vec<Vec<u8>>, bool),
        DcsHook(Params, Vec<u8>, u8),
        DcsPut(u8),
        DcsUnhook(bool),
        StartString(u8),
    }

    #[derive(Default)]
    struct TraceHandler {
        events: Vec<Event>,
    }

    impl VtHandler for TraceHandler {
        fn print(&mut self, ch: char) {
            self.events.push(Event::Print(ch));
        }

        fn execute(&mut self, byte: u8) {
            self.events.push(Event::Execute(byte));
        }

        fn csi_dispatch(
            &mut self,
            params: &Params,
            intermediates: &[u8],
            action: u8,
            private_marker: Option<u8>,
        ) {
            self.events.push(Event::Csi(
                *params,
                intermediates.to_vec(),
                action,
                private_marker,
            ));
        }

        fn esc_dispatch(&mut self, intermediates: &[u8], byte: u8) {
            self.events.push(Event::Esc(intermediates.to_vec(), byte));
        }

        fn osc_dispatch(&mut self, params: &[&[u8]], bell_terminated: bool) {
            self.events.push(Event::Osc(
                params.iter().map(|param| param.to_vec()).collect(),
                bell_terminated,
            ));
        }

        fn dcs_hook(&mut self, params: &Params, intermediates: &[u8], action: u8) {
            self.events
                .push(Event::DcsHook(*params, intermediates.to_vec(), action));
        }

        fn dcs_put(&mut self, byte: u8) {
            self.events.push(Event::DcsPut(byte));
        }

        fn dcs_unhook(&mut self, terminated: bool) {
            self.events.push(Event::DcsUnhook(terminated));
        }

        fn start_string(&mut self, kind: u8) {
            self.events.push(Event::StartString(kind));
        }
    }

    fn trace_contiguous(payload: &[u8], c1_enabled: bool) -> Vec<Event> {
        let mut parser = Parser::default();
        parser.set_c1_enabled(c1_enabled);
        let mut handler = TraceHandler::default();
        for &byte in payload {
            parser.advance(&mut handler, byte);
        }
        handler.events
    }

    fn trace_scheduled(payload: &[u8], schedule: &[u8], c1_enabled: bool) -> Vec<Event> {
        let mut parser = Parser::default();
        parser.set_c1_enabled(c1_enabled);
        let mut handler = TraceHandler::default();
        for_each_scheduled_chunk(schedule, payload, |chunk| {
            for &byte in chunk {
                parser.advance(&mut handler, byte);
            }
        });
        handler.events
    }

    #[test]
    fn scheduled_parser_feed_matches_contiguous_feed_at_arbitrary_boundaries() {
        let payload = b"abc\x1b[1;2m\x1b]0;title\x07\x1bPqdata\x1b\\visible";
        let schedule = [0, 1, 255, 0, 2, 3];

        for c1_enabled in [false, true] {
            assert_eq!(
                trace_scheduled(payload, &schedule, c1_enabled),
                trace_contiguous(payload, c1_enabled),
                "scheduled feed diverged with c1_enabled={c1_enabled}"
            );
        }
    }

    #[test]
    fn decoder_uses_all_short_inputs_as_payload() {
        for length in [1usize, 31, 32] {
            let data: Vec<u8> = (0..length as u8).collect();
            let (schedule, payload) = decode_schedule_payload(&data);
            assert!(schedule.is_empty(), "length={length}");
            assert_eq!(payload, data.as_slice(), "length={length}");
        }
    }

    #[test]
    fn decoder_reserves_first_32_bytes_for_long_inputs() {
        for length in [33usize, 64] {
            let data: Vec<u8> = (0..length as u8).collect();
            let (schedule, payload) = decode_schedule_payload(&data);
            assert_eq!(schedule, &data[..32], "length={length}");
            assert_eq!(payload, &data[32..], "length={length}");
            assert!(!payload.is_empty(), "length={length}");
        }
    }

    #[test]
    fn decoder_keeps_empty_input_empty() {
        let (schedule, payload) = decode_schedule_payload(&[]);
        assert!(schedule.is_empty());
        assert!(payload.is_empty());
    }

    #[test]
    fn scheduled_chunks_consume_payload_with_empty_or_short_schedule() {
        let mut chunks = Vec::new();
        for_each_scheduled_chunk(&[], b"abc", |chunk| chunks.push(chunk.to_vec()));
        assert_eq!(chunks, vec![b"a".to_vec(), b"b".to_vec(), b"c".to_vec()]);

        chunks.clear();
        for_each_scheduled_chunk(&[0, 2], b"abcde", |chunk| chunks.push(chunk.to_vec()));
        assert_eq!(
            chunks,
            vec![b"a".to_vec(), b"bc".to_vec(), b"d".to_vec(), b"e".to_vec()]
        );
    }

    #[test]
    fn checked_in_corpus_decodes_schedule_without_eating_protocol_payload() {
        let corpus: &[(&[u8], &[u8])] = &[
            (include_bytes!("../corpus/parser/csi-nested-esc"), b"\x1b["),
            (
                include_bytes!("../corpus/parser/osc-over-limit-st"),
                b"\x1b]",
            ),
            (
                include_bytes!("../corpus/parser/dcs-over-limit-can"),
                b"\x1bP",
            ),
            (
                include_bytes!("../corpus/parser/apc-over-limit-st"),
                b"\x1b_",
            ),
            (
                include_bytes!("../corpus/parser/pm-over-limit-sub"),
                b"\x1b^",
            ),
            (
                include_bytes!("../corpus/parser/sos-over-limit-st"),
                b"\x1bX",
            ),
            (
                include_bytes!("../corpus/parser/utf8-fragmentation"),
                &[0xf0, 0x9f],
            ),
            (
                include_bytes!("../corpus/parser/can-sub-recovery"),
                b"\x1b]",
            ),
        ];
        let expected_schedule: Vec<u8> = (1..=32).collect();

        for (data, payload_prefix) in corpus {
            let (schedule, payload) = decode_schedule_payload(data);
            assert_eq!(schedule, expected_schedule.as_slice());
            assert!(payload.starts_with(payload_prefix));
            assert!(payload.ends_with(b"Z"));

            let mut consumed = 0;
            for_each_scheduled_chunk(schedule, payload, |chunk| consumed += chunk.len());
            assert_eq!(consumed, payload.len());
        }
    }
}

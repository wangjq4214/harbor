use crate::ShellIntegrationMarker;

const MAX_PAYLOAD_BYTES: usize = 1024;

pub(super) enum Action {
    Marker(ShellIntegrationMarker),
    Reset,
}

pub(super) fn action(payload: &[u8]) -> Option<Action> {
    if payload.is_empty() {
        Some(Action::Reset)
    } else {
        parse(payload).map(Action::Marker)
    }
}

/// Parses the OSC 133 semantic prompt / shell-integration payload.
///
/// `payload` is the slice immediately following `133;`, without the leading semicolon.
/// Supports markers `A`, `B`, `C`, and `D` (with optional integer exit code).
/// Unknown subcommands or malformed payloads return `None` (consume-ignore).
pub(super) fn parse(payload: &[u8]) -> Option<ShellIntegrationMarker> {
    if payload.len() > MAX_PAYLOAD_BYTES || payload.iter().any(|b| b.is_ascii_control()) {
        return None;
    }

    match payload {
        b"A" => Some(ShellIntegrationMarker::PromptStart),
        b"B" => Some(ShellIntegrationMarker::PromptEnd),
        b"C" => Some(ShellIntegrationMarker::CommandExecuted),
        b"D" => Some(ShellIntegrationMarker::CommandFinished(None)),
        _ => {
            let code_bytes = payload.strip_prefix(b"D;")?;
            let code = std::str::from_utf8(code_bytes).ok()?.parse::<i32>().ok()?;
            Some(ShellIntegrationMarker::CommandFinished(Some(code)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signed_exit_code_boundaries_and_exact_payloads() {
        for (payload, code) in [
            (b"D;+1".as_slice(), 1),
            (b"D;-2147483648", i32::MIN),
            (b"D;2147483647", i32::MAX),
            (b"D;000", 0),
        ] {
            assert_eq!(
                parse(payload),
                Some(ShellIntegrationMarker::CommandFinished(Some(code)))
            );
        }
        for payload in [
            b"A;".as_slice(),
            b"B;",
            b"C;",
            b"D;",
            b"D;;",
            b"D;0;",
            b"D;2147483648",
            b"D;-2147483649",
            b"D; 0",
            b"D;\xff",
        ] {
            assert_eq!(parse(payload), None, "accepted {payload:?}");
        }
    }
    #[test]
    fn parse_valid_subcommands() {
        assert_eq!(parse(b"A"), Some(ShellIntegrationMarker::PromptStart));
        assert_eq!(parse(b"B"), Some(ShellIntegrationMarker::PromptEnd));
        assert_eq!(parse(b"C"), Some(ShellIntegrationMarker::CommandExecuted));
        assert_eq!(
            parse(b"D"),
            Some(ShellIntegrationMarker::CommandFinished(None))
        );
        assert_eq!(
            parse(b"D;0"),
            Some(ShellIntegrationMarker::CommandFinished(Some(0)))
        );
        assert_eq!(
            parse(b"D;130"),
            Some(ShellIntegrationMarker::CommandFinished(Some(130)))
        );
        assert_eq!(
            parse(b"D;-1"),
            Some(ShellIntegrationMarker::CommandFinished(Some(-1)))
        );
    }

    #[test]
    fn reject_extra_arguments_and_malformed_exit_codes() {
        assert_eq!(parse(b"A;cl=m"), None);
        assert_eq!(parse(b"B;aid=1"), None);
        assert_eq!(parse(b"C;timestamp=123"), None);
        assert_eq!(parse(b"D;"), None);
        assert_eq!(parse(b"D;0;aid=foo"), None);
        assert_eq!(parse(b"D;invalid_code"), None);
    }
    #[test]
    fn ignore_unknown_subcommands_and_invalid_payloads() {
        assert_eq!(parse(b""), None);
        assert_eq!(parse(b"E"), None);
        assert_eq!(parse(b"P"), None);
        assert_eq!(parse(b"?"), None);
        assert_eq!(parse(b"Afoo"), None);
        assert_eq!(parse(b"D_"), None);
        assert_eq!(parse(b"A\x00"), None);
        assert_eq!(parse(b"A\n"), None);

        let huge = vec![b'A'; MAX_PAYLOAD_BYTES + 1];
        assert_eq!(parse(&huge), None);
    }
}

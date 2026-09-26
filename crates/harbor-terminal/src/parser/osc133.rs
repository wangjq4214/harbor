use crate::ShellIntegrationMarker;

const MAX_PAYLOAD_BYTES: usize = 1024;

/// Parses the OSC 133 semantic prompt / shell-integration payload.
///
/// `payload` is the slice immediately following `133;`, without the leading semicolon.
/// Supports markers `A`, `B`, `C`, and `D` (with optional integer exit code).
/// Unknown subcommands or malformed payloads return `None` (consume-ignore).
pub(super) fn parse(payload: &[u8]) -> Option<ShellIntegrationMarker> {
    if payload.len() > MAX_PAYLOAD_BYTES || payload.iter().any(|b| b.is_ascii_control()) {
        return None;
    }

    let mut parts = payload.split(|b| *b == b';');
    let subcommand = parts.next()?;

    match subcommand {
        b"A" | b"B" | b"C" => {
            let marker = match subcommand {
                b"A" => ShellIntegrationMarker::PromptStart,
                b"B" => ShellIntegrationMarker::PromptEnd,
                b"C" => ShellIntegrationMarker::CommandExecuted,
                _ => unreachable!(),
            };
            parts.next().is_none().then_some(marker)
        }
        b"D" => match parts.next() {
            None => Some(ShellIntegrationMarker::CommandFinished(None)),
            Some(code_bytes) => {
                if parts.next().is_some() || code_bytes.is_empty() {
                    return None;
                }
                let s = std::str::from_utf8(code_bytes).ok()?;
                let code = s.parse::<i32>().ok()?;
                Some(ShellIntegrationMarker::CommandFinished(Some(code)))
            }
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

const MAX_URI_BYTES: usize = 2048;
const MAX_ID_BYTES: usize = 250;

#[derive(Debug, Eq, PartialEq)]
pub(super) enum Action {
    Open { uri: String, id: Option<String> },
    Close,
}

/// Parses the terminal-layer OSC 8 payload (`params;URI`).
pub(super) fn parse(payload: &[u8]) -> Option<Action> {
    let separator = payload.iter().position(|byte| *byte == b';')?;
    let (params, uri_with_separator) = payload.split_at(separator);
    let uri = &uri_with_separator[1..];

    if uri.is_empty() {
        return Some(Action::Close);
    }
    let uri = bounded_control_free_utf8(uri, MAX_URI_BYTES)?.to_owned();

    let mut id = None;
    for param in params.split(|byte| *byte == b':') {
        if let Some(value) = param.strip_prefix(b"id=") {
            if id.is_some() {
                return None;
            }
            id = Some(bounded_control_free_utf8(value, MAX_ID_BYTES)?.to_owned());
        } else if param == b"id" {
            return None;
        }
    }

    Some(Action::Open { uri, id })
}

fn bounded_control_free_utf8(bytes: &[u8], limit: usize) -> Option<&str> {
    if bytes.len() > limit {
        return None;
    }
    let value = std::str::from_utf8(bytes).ok()?;
    (!value.chars().any(char::is_control)).then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_open_close_and_retains_uri_semicolons() {
        assert_eq!(
            parse(b"id=abc:unknown=value;https://example.test/a;b"),
            Some(Action::Open {
                uri: "https://example.test/a;b".to_owned(),
                id: Some("abc".to_owned()),
            })
        );
        assert_eq!(parse(b"anything;"), Some(Action::Close));
    }

    #[test]
    fn rejects_invalid_candidates_and_accepts_exact_limits() {
        let uri = vec![b'a'; MAX_URI_BYTES];
        let mut payload = b";".to_vec();
        payload.extend_from_slice(&uri);
        assert!(matches!(parse(&payload), Some(Action::Open { .. })));
        payload.push(b'a');
        assert_eq!(parse(&payload), None);

        let id = vec![b'i'; MAX_ID_BYTES];
        let mut payload = b"id=".to_vec();
        payload.extend_from_slice(&id);
        payload.extend_from_slice(b";https://example.test");
        assert!(matches!(parse(&payload), Some(Action::Open { .. })));
        payload.insert(3 + MAX_ID_BYTES, b'i');
        assert_eq!(parse(&payload), None);

        assert_eq!(parse(b"id=a:id=b;https://example.test"), None);
        assert_eq!(parse(b"id;https://example.test"), None);
        assert_eq!(parse(b";bad\0uri"), None);
        assert_eq!(parse(b";\xff"), None);
        assert_eq!(parse(b"missing-separator"), None);
    }
}

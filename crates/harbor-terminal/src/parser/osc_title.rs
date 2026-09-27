/// Validated title change or explicit empty-payload reset for OSC 0/1/2.
pub(super) enum Action {
    Change(String),
    Reset,
}

pub(super) fn parse(payload: &[u8]) -> Option<Action> {
    if payload.is_empty() {
        return Some(Action::Reset);
    }
    let title = std::str::from_utf8(payload).ok()?;
    if title.chars().take(257).count() > 256 || title.chars().any(char::is_control) {
        return None;
    }
    Some(Action::Change(title.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounds_and_validates_title_without_truncation() {
        assert!(matches!(parse(b""), Some(Action::Reset)));
        assert!(matches!(
            parse("界".repeat(256).as_bytes()),
            Some(Action::Change(_))
        ));
        assert!(parse("界".repeat(257).as_bytes()).is_none());
        assert!(parse(b"bad\x00title").is_none());
        assert!(parse(b"\xff").is_none());
    }
}

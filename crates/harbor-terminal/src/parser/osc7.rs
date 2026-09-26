use crate::WorkingDirectoryMetadata;

const SCHEME: &[u8] = b"file://";
const MAX_HOST_BYTES: usize = 255;
const MAX_PATH_BYTES: usize = 2048;

/// Parses the deliberately narrow OSC 7 file-URI contract owned by the terminal layer.
pub(super) fn parse(payload: &[u8]) -> Option<WorkingDirectoryMetadata> {
    let remainder = payload.strip_prefix(SCHEME)?;
    let slash = remainder.iter().position(|byte| *byte == b'/')?;
    let (authority, raw_path) = remainder.split_at(slash);

    if !valid_host(authority) || raw_path.contains(&b'?') || raw_path.contains(&b'#') {
        return None;
    }

    let path_bytes = strict_percent_decode(raw_path)?;
    let path = std::str::from_utf8(&path_bytes).ok()?;
    if path.chars().any(char::is_control) {
        return None;
    }

    let host = if authority.is_empty() {
        None
    } else {
        Some(std::str::from_utf8(authority).ok()?.to_owned())
    };
    Some(WorkingDirectoryMetadata {
        host,
        path: path.to_owned(),
    })
}

fn valid_host(authority: &[u8]) -> bool {
    if authority.len() > MAX_HOST_BYTES {
        return false;
    }

    let mut index = 0;
    while index < authority.len() {
        let byte = authority[index];
        if byte == b'%' {
            if authority
                .get(index + 1..index + 3)
                .is_none_or(|digits| digits.iter().any(|digit| hex(*digit).is_none()))
            {
                return false;
            }
            index += 3;
        } else if is_unreserved_or_sub_delim(byte) {
            index += 1;
        } else {
            return false;
        }
    }
    true
}

fn is_unreserved_or_sub_delim(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
        || matches!(
            byte,
            b'-' | b'.'
                | b'_'
                | b'~'
                | b'!'
                | b'$'
                | b'&'
                | b'\''
                | b'('
                | b')'
                | b'*'
                | b'+'
                | b','
                | b';'
                | b'='
        )
}

fn is_raw_path_byte(byte: u8) -> bool {
    is_unreserved_or_sub_delim(byte) || matches!(byte, b':' | b'@' | b'/')
}

fn strict_percent_decode(raw: &[u8]) -> Option<Vec<u8>> {
    let mut decoded = Vec::with_capacity(raw.len().min(MAX_PATH_BYTES));
    let mut index = 0;
    while index < raw.len() {
        let byte = if raw[index] == b'%' {
            let high = hex(*raw.get(index + 1)?)?;
            let low = hex(*raw.get(index + 2)?)?;
            index += 3;
            (high << 4) | low
        } else {
            let byte = raw[index];
            if !is_raw_path_byte(byte) {
                return None;
            }
            index += 1;
            byte
        };
        if decoded.len() == MAX_PATH_BYTES {
            return None;
        }
        decoded.push(byte);
    }
    Some(decoded)
}

fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metadata(host: Option<&str>, path: &str) -> WorkingDirectoryMetadata {
        WorkingDirectoryMetadata {
            host: host.map(str::to_owned),
            path: path.to_owned(),
        }
    }

    #[test]
    fn accepts_local_remote_unicode_and_windows_uri_paths() {
        assert_eq!(parse(b"file:///"), Some(metadata(None, "/")));
        assert_eq!(
            parse(b"file://host/home/Alice"),
            Some(metadata(Some("host"), "/home/Alice"))
        );
        assert_eq!(
            parse(b"file:///C:/Users/Alice"),
            Some(metadata(None, "/C:/Users/Alice"))
        );
        assert_eq!(
            parse(b"file:///hello%20%E4%B8%96%E7%95%8C%2520"),
            Some(metadata(None, "/hello 世界%20"))
        );
    }

    #[test]
    fn enforces_host_and_decoded_path_limits() {
        let host_255 = "h".repeat(255);
        let host_256 = "h".repeat(256);
        let path_2048 = format!("/{}", "p".repeat(2047));
        let path_2049 = format!("/{}", "p".repeat(2048));

        assert!(parse(format!("file://{host_255}/x").as_bytes()).is_some());
        assert!(parse(format!("file://{host_256}/x").as_bytes()).is_none());
        assert!(parse(format!("file://{path_2048}").as_bytes()).is_some());
        assert!(parse(format!("file://{path_2049}").as_bytes()).is_none());
    }

    #[test]
    fn rejects_unsupported_uri_forms_and_invalid_content() {
        for invalid in [
            b"http:///tmp".as_slice(),
            b"file://host".as_slice(),
            b"file://bad%host/tmp".as_slice(),
            b"file://bad\\host/tmp".as_slice(),
            b"file://user@host/tmp".as_slice(),
            b"file://host:22/tmp".as_slice(),
            b"file://host/tmp?query".as_slice(),
            b"file://host/tmp#fragment".as_slice(),
            b"file:///bad%".as_slice(),
            b"file:///bad%2".as_slice(),
            b"file:///bad%xx".as_slice(),
            b"file://ho st/tmp".as_slice(),
            b"file:///raw space".as_slice(),
            b"file:///raw\\backslash".as_slice(),
            b"file:///raw-\xe4\xb8\x96\xe7\x95\x8c".as_slice(),
            b"file:///bad%00path".as_slice(),
            b"file:///bad%7fpath".as_slice(),
            b"file:///bad%c2%80path".as_slice(),
            b"file:///bad%ffpath".as_slice(),
        ] {
            assert_eq!(parse(invalid), None, "accepted {invalid:?}");
        }
    }
}

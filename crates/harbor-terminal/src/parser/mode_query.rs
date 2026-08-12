use crate::screen::ModeStatus;
use harbor_parser::Params;

/// DECRQM request validation and DECRPM reply formatting.
pub(crate) struct ModeQuery;

impl ModeQuery {
    /// Returns the queried mode number for a single, nonempty parameter.
    pub(crate) fn param(params: &Params) -> Option<usize> {
        (params.len() == 1 && params.sub_params_len(0) == Some(1))
            .then(|| params.get(0))
            .flatten()
    }

    pub(crate) fn reply(param: usize, status: ModeStatus, private: bool) -> Vec<u8> {
        let prefix = if private { "?" } else { "" };
        format!("\x1b[{prefix}{param};{}$y", status.code()).into_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_exactly_one_nonempty_parameter() {
        let valid = Params::from(&[Some(7)][..]);
        let empty = Params::from(&[None][..]);
        let multiple = Params::from(&[Some(7), Some(8)][..]);

        assert_eq!(ModeQuery::param(&valid), Some(7));
        assert_eq!(ModeQuery::param(&empty), None);
        assert_eq!(ModeQuery::param(&multiple), None);
    }

    #[test]
    fn formats_standard_and_private_mode_reports() {
        assert_eq!(ModeQuery::reply(4, ModeStatus::Set, false), b"\x1b[4;1$y");
        assert_eq!(
            ModeQuery::reply(2004, ModeStatus::Reset, true),
            b"\x1b[?2004;2$y"
        );
    }

    #[test]
    fn formats_each_decrpm_status_code() {
        assert_eq!(
            ModeQuery::reply(1, ModeStatus::Unknown, false),
            b"\x1b[1;0$y"
        );
        assert_eq!(ModeQuery::reply(1, ModeStatus::Set, false), b"\x1b[1;1$y");
        assert_eq!(ModeQuery::reply(1, ModeStatus::Reset, false), b"\x1b[1;2$y");
        assert_eq!(
            ModeQuery::reply(1, ModeStatus::PermanentlySet, false),
            b"\x1b[1;3$y"
        );
        assert_eq!(
            ModeQuery::reply(1, ModeStatus::PermanentlyReset, false),
            b"\x1b[1;4$y"
        );
    }
}

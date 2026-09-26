use harbor_parser::Params;

/// Primary and secondary device attributes advertised by Harbor.
pub(crate) const PRIMARY_REPLY: &[u8] = b"\x1b[?62;6;17;22;28c";
pub(crate) const SECONDARY_REPLY: &[u8] = b"\x1b[>1;1;0c";

pub(crate) fn accepts_default_query(params: &Params) -> bool {
    params.len() == 1
        && params.sub_params_len(0) == Some(1)
        && matches!(params.get(0), None | Some(0))
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replies_have_stable_capabilities_and_identity() {
        assert_eq!(PRIMARY_REPLY, b"\x1b[?62;6;17;22;28c");
        assert_eq!(SECONDARY_REPLY, b"\x1b[>1;1;0c");
    }

    #[test]
    fn only_omitted_and_zero_parameters_are_accepted() {
        let omitted = Params::from(&[None][..]);
        let zero = Params::from(&[Some(0)][..]);
        let nonzero = Params::from(&[Some(1)][..]);
        let multiple = Params::from(&[Some(0), Some(0)][..]);

        assert!(accepts_default_query(&omitted));
        assert!(accepts_default_query(&zero));
        assert!(!accepts_default_query(&nonzero));
        assert!(!accepts_default_query(&multiple));
    }
}

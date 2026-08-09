use harbor_parser::Params;

/// Primary Device Attributes supported by Harbor.
///
/// The registry is intentionally private to the terminal parser so replies can only
/// advertise capabilities backed by terminal behavior in this crate.
pub(crate) struct PrimaryDeviceAttributes;

impl PrimaryDeviceAttributes {
    const MODEL: usize = 62;
    const CAPABILITIES: [usize; 4] = [6, 17, 22, 28];

    pub(crate) fn accepts(params: &Params) -> bool {
        accepts_default_query(params)
    }

    pub(crate) fn reply() -> Vec<u8> {
        let mut reply = format!("\x1b[?{}", Self::MODEL);
        for capability in Self::CAPABILITIES {
            reply.push(';');
            reply.push_str(&capability.to_string());
        }
        reply.push('c');
        reply.into_bytes()
    }
}

/// Secondary Device Attributes identity kept stable across ordinary Harbor releases.
pub(crate) struct SecondaryDeviceAttributes;

impl SecondaryDeviceAttributes {
    const PRODUCT: usize = 1;
    const REVISION: usize = 1;
    const OPTION: usize = 0;

    pub(crate) fn accepts(params: &Params) -> bool {
        accepts_default_query(params)
    }

    pub(crate) fn reply() -> Vec<u8> {
        format!(
            "\x1b[>{};{};{}c",
            Self::PRODUCT,
            Self::REVISION,
            Self::OPTION
        )
        .into_bytes()
    }
}

fn accepts_default_query(params: &Params) -> bool {
    params.len() == 1
        && params.sub_params_len(0) == Some(1)
        && matches!(params.get(0), None | Some(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primary_reply_is_derived_from_registry() {
        assert_eq!(PrimaryDeviceAttributes::reply(), b"\x1b[?62;6;17;22;28c");
    }

    #[test]
    fn secondary_reply_has_stable_identity() {
        assert_eq!(SecondaryDeviceAttributes::reply(), b"\x1b[>1;1;0c");
    }

    #[test]
    fn only_omitted_and_zero_parameters_are_accepted() {
        let omitted = Params::from(&[None][..]);
        let zero = Params::from(&[Some(0)][..]);
        let nonzero = Params::from(&[Some(1)][..]);
        let multiple = Params::from(&[Some(0), Some(0)][..]);

        assert!(PrimaryDeviceAttributes::accepts(&omitted));
        assert!(PrimaryDeviceAttributes::accepts(&zero));
        assert!(!PrimaryDeviceAttributes::accepts(&nonzero));
        assert!(!PrimaryDeviceAttributes::accepts(&multiple));
        assert!(SecondaryDeviceAttributes::accepts(&omitted));
        assert!(SecondaryDeviceAttributes::accepts(&zero));
    }
}

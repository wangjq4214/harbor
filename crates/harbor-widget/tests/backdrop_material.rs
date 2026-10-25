use harbor_widget::{
    BackdropFilter, BackdropMaterial, BackdropRim, BackdropTone, DecorationError, EdgeRefraction,
};

const IDENTITY: BackdropTone = BackdropTone {
    offset: 0.0,
    slope: 1.0,
    quadratic: 0.0,
    saturation: 1.0,
};
const NO_RIM: BackdropRim = BackdropRim {
    highlight: 0.0,
    shade: 0.0,
};

fn material_with(field: &str, value: f32) -> Result<BackdropMaterial, DecorationError> {
    let (mut sigma, mut mix, mut tone, mut rim) = (1.0, 0.0, IDENTITY, NO_RIM);
    *match field {
        "material.fill_sigma" => &mut sigma,
        "material.fill_mix" => &mut mix,
        "material.tone.offset" => &mut tone.offset,
        "material.tone.slope" => &mut tone.slope,
        "material.tone.quadratic" => &mut tone.quadratic,
        "material.tone.saturation" => &mut tone.saturation,
        "material.rim.highlight" => &mut rim.highlight,
        "material.rim.shade" => &mut rim.shade,
        _ => panic!("unknown test field: {field}"),
    } = value;
    BackdropMaterial::new(sigma, mix, tone, rim)
}

#[test]
fn every_raw_material_field_is_validated_at_consumption_with_exact_diagnostics() {
    for (field, nonnegative) in [
        ("material.fill_sigma", true),
        ("material.fill_mix", false),
        ("material.tone.offset", false),
        ("material.tone.slope", false),
        ("material.tone.quadratic", false),
        ("material.tone.saturation", true),
        ("material.rim.highlight", true),
        ("material.rim.shade", true),
    ] {
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let error = material_with(field, value).unwrap_err();
            assert_eq!(error, DecorationError::NonFinite { field });
            assert_eq!(error.to_string(), format!("{field} must be finite"));
        }
        for value in [-1.0, -f32::MAX] {
            let result = material_with(field, value);
            if nonnegative {
                let error = result.unwrap_err();
                assert_eq!(error, DecorationError::Negative { field });
                assert_eq!(error.to_string(), format!("{field} must be non-negative"));
            } else {
                assert!(result.is_ok(), "signed {field} must accept {value}");
            }
        }
        for value in [-0.0, 0.0, 0.25, f32::MAX] {
            assert!(
                material_with(field, value).is_ok(),
                "{field} must accept {value}"
            );
        }
    }
}

#[test]
fn finite_material_values_keep_the_existing_caps_and_signed_clamps() {
    for value in [-f32::MAX, -3.0, -2.0, -0.25, 0.0, 0.25, 2.0, 3.0, f32::MAX] {
        let material = BackdropMaterial::new(
            f32::MAX,
            value,
            BackdropTone {
                offset: value,
                slope: value,
                quadratic: value,
                saturation: f32::MAX,
            },
            BackdropRim {
                highlight: f32::MAX,
                shade: f32::MAX,
            },
        )
        .unwrap();
        assert_eq!(material.frost(), [64.0, value.clamp(-1.0, 1.0)]);
        let coefficient = value.clamp(-2.0, 2.0);
        assert_eq!(
            material.tone(),
            [coefficient, coefficient, coefficient, 2.0]
        );
        assert_eq!(material.rim(), [1.0, 1.0]);
    }
    let zero = BackdropMaterial::new(
        0.0,
        0.0,
        BackdropTone {
            offset: 0.0,
            slope: 0.0,
            quadratic: 0.0,
            saturation: 0.0,
        },
        NO_RIM,
    )
    .unwrap();
    assert_eq!(zero.frost(), [0.0; 2]);
    assert_eq!(zero.tone(), [0.0; 4]);
    assert_eq!(zero.rim(), [0.0; 2]);

    let exact = BackdropMaterial::new(
        12.5,
        -0.3,
        BackdropTone {
            offset: 0.12,
            slope: -0.34,
            quadratic: 0.56,
            saturation: 0.78,
        },
        BackdropRim {
            highlight: 0.9,
            shade: 0.21,
        },
    )
    .unwrap();
    assert_eq!(exact.frost(), [12.5, -0.3]);
    assert_eq!(exact.tone(), [0.12, -0.34, 0.56, 0.78]);
    assert_eq!(exact.rim(), [0.9, 0.21]);
}

#[test]
fn previously_consumed_named_inputs_are_not_trusted_after_mutation() {
    let mut tone = IDENTITY;
    let mut rim = NO_RIM;
    assert!(BackdropMaterial::new(0.0, 0.0, tone, rim).is_ok());
    tone.quadratic = f32::NAN;
    assert_eq!(
        BackdropMaterial::new(0.0, 0.0, tone, rim),
        Err(DecorationError::NonFinite {
            field: "material.tone.quadratic"
        }),
    );
    tone = IDENTITY;
    rim.shade = -0.1;
    assert_eq!(
        BackdropMaterial::new(0.0, 0.0, tone, rim),
        Err(DecorationError::Negative {
            field: "material.rim.shade"
        }),
    );
}

#[test]
fn blur_and_refraction_retain_their_raw_domains_and_caps() {
    type Entry = fn(f32) -> Result<(), DecorationError>;
    let entries: [(&str, Entry); 3] = [
        ("blur.sigma", |v| BackdropFilter::new(v).map(|_| ())),
        ("refraction.width", |v| {
            EdgeRefraction::new(v, 1.0).map(|_| ())
        }),
        ("refraction.displacement", |v| {
            EdgeRefraction::new(1.0, v).map(|_| ())
        }),
    ];
    for (field, entry) in entries {
        for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert_eq!(entry(invalid), Err(DecorationError::NonFinite { field }));
        }
        assert_eq!(entry(-1.0), Err(DecorationError::Negative { field }));
        for valid in [-0.0, 0.0, 0.5, f32::MAX] {
            assert!(entry(valid).is_ok());
        }
    }
    let refraction = EdgeRefraction::new(f32::MAX, f32::MAX).unwrap();
    assert_eq!(refraction.width(), 64.0);
    assert_eq!(refraction.displacement(), 32.0);
}

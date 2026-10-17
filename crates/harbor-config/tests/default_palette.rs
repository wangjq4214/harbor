use harbor_config::{Color, Palette, Rgba, Settings, load_from_path};

#[test]
fn default_terminal_palette_is_catppuccin_mocha_with_more_opaque_background() {
    let palette = Settings::default().colors;
    assert_eq!(palette, Palette::default());
    assert_eq!(palette.foreground, "#CDD6F4".parse::<Rgba>().unwrap());
    let background = palette.background.components();
    let base = "#1E1E2E".parse::<Rgba>().unwrap().components();
    assert_eq!(background[..3], base[..3]);
    assert_eq!(background[3], 0.85);
    assert_eq!(palette.cursor, "#F5E0DCCC".parse::<Rgba>().unwrap());
    assert_eq!(palette.selection, "#F5E0DC66".parse::<Rgba>().unwrap());
    assert_eq!(palette.background.components(), harbor_config::BACKGROUND);
    assert_eq!(
        palette.selection.components(),
        harbor_config::SELECTION_COLOR
    );

    let normal = [
        "#45475A", "#F38BA8", "#A6E3A1", "#F9E2AF", "#89B4FA", "#F5C2E7", "#94E2D5", "#BAC2DE",
    ];
    let bright = [
        "#585B70", "#F38BA8", "#A6E3A1", "#F9E2AF", "#89B4FA", "#F5C2E7", "#94E2D5", "#A6ADC8",
    ];
    assert_eq!(
        palette.normal,
        normal.map(|hex| hex.parse::<Rgba>().unwrap())
    );
    assert_eq!(
        palette.bright,
        bright.map(|hex| hex.parse::<Rgba>().unwrap())
    );
}

#[test]
fn mocha_keeps_ansi_aliases_and_extended_xterm_colors() {
    let palette = Palette::default();
    assert_eq!(
        palette.resolve(Color::Default),
        palette.foreground.components()
    );
    for index in 0..8 {
        assert_eq!(
            palette.resolve(Color::Named(index)),
            palette.resolve(Color::Indexed(index))
        );
        assert_eq!(
            palette.resolve(Color::Bright(index)),
            palette.resolve(Color::Indexed(index + 8))
        );
    }
    for (index, hex) in [
        (16, "#000000"),
        (17, "#00005F"),
        (21, "#0000FF"),
        (231, "#FFFFFF"),
        (232, "#080808"),
        (255, "#EEEEEE"),
    ] {
        assert_eq!(palette.indexed(index), hex.parse::<Rgba>().unwrap());
    }
    assert_eq!(
        palette.resolve(Color::Rgb(1, 2, 3)),
        Rgba::from_rgb8(1, 2, 3).components()
    );
}

#[test]
fn example_configuration_uses_the_default_terminal_colors() {
    let loaded = load_from_path(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../config.example.toml"
    ));
    assert!(loaded.diagnostics.is_empty(), "{:?}", loaded.diagnostics);
    let defaults = Palette::default();
    let example = loaded.settings.colors;
    assert_eq!(example.foreground, defaults.foreground);
    assert_eq!(example.cursor, defaults.cursor);
    assert_eq!(example.selection, defaults.selection);
    assert_eq!(example.normal, defaults.normal);
    assert_eq!(example.bright, defaults.bright);
    assert_eq!(example.extended, defaults.extended);
    assert_eq!(
        example.background.components()[..3],
        defaults.background.components()[..3]
    );
    assert_eq!(example.background, defaults.background);
}

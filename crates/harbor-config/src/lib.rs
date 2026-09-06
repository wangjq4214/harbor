//! Startup settings and application-wide behavior constants.
//!
//! User settings are loaded once from `~/.harbor/config.toml`. Missing or
//! unreadable files and invalid TOML fall back to complete defaults. Once TOML
//! parses, ordinary fields recover independently while the color palette is
//! validated and committed atomically.

use std::{
    fs,
    path::{Path, PathBuf},
};

use harbor_types::{Palette, Rgba};
use toml::{Table, Value};

pub const FONT_SIZE: f32 = 24.0;
pub const TEXT_PADDING: f32 = 16.0;
pub const BACKGROUND: [f32; 4] = [0.36, 0.20, 0.08, 0.25];
pub const SELECTION_COLOR: [f32; 4] = [0.3, 0.5, 0.9, 0.4];
pub const BLINK_INTERVAL_MS: u64 = 530;
pub const SCROLLBAR_WIDTH: f32 = 6.0;
pub const SCROLLBAR_MARGIN: f32 = 2.0;
pub const SCROLLBAR_COLOR: [f32; 4] = [0.8, 0.8, 0.8, 0.4];
pub const SCROLLBAR_HIDE_DELAY_MS: u64 = 1500;
pub const SCROLLBAR_MIN_THUMB_HEIGHT: f32 = 20.0;
pub const SCROLLBAR_BORDER_RADIUS: f32 = 3.0;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Settings {
    pub font: FontSettings,
    pub shell: ShellSettings,
    pub colors: Palette,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FontSettings {
    /// Requested DirectWrite family. `None` delegates primary selection to Windows.
    pub family: Option<String>,
    pub size: f32,
}

impl Default for FontSettings {
    fn default() -> Self {
        Self {
            family: None,
            size: FONT_SIZE,
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ShellSettings {
    /// Requested executable. `None` preserves the platform COMSPEC/cmd fallback.
    pub program: Option<String>,
    pub args: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiagnosticLevel {
    Warning,
    Error,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Diagnostic {
    pub level: DiagnosticLevel,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SettingsLoad {
    pub settings: Settings,
    pub diagnostics: Vec<Diagnostic>,
}

impl SettingsLoad {
    fn defaults(message: impl Into<String>) -> Self {
        Self {
            settings: Settings::default(),
            diagnostics: vec![Diagnostic {
                level: DiagnosticLevel::Error,
                message: message.into(),
            }],
        }
    }
}

/// Resolves Harbor's configuration path without creating directories or files.
pub fn default_config_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".harbor").join("config.toml"))
}

/// Loads startup settings from Harbor's default per-user path.
pub fn load() -> SettingsLoad {
    match default_config_path() {
        Some(path) => load_from_path(path),
        None => SettingsLoad::defaults("could not resolve the user home directory"),
    }
}

/// Loads startup settings from an injectable path.
pub fn load_from_path(path: impl AsRef<Path>) -> SettingsLoad {
    let path = path.as_ref();
    let source = match fs::read_to_string(path) {
        Ok(source) => source,
        Err(error) => {
            return SettingsLoad::defaults(format!(
                "could not read settings {}: {error}",
                path.display()
            ));
        }
    };
    match toml::from_str::<Value>(&source) {
        Ok(document) => parse_document(document),
        Err(error) => SettingsLoad::defaults(format!(
            "could not parse settings {}: {error}",
            path.display()
        )),
    }
}

fn parse_document(document: Value) -> SettingsLoad {
    let mut diagnostics = Vec::new();
    let Some(root) = document.as_table() else {
        return SettingsLoad::defaults("settings document must be a TOML table");
    };
    warn_unknown(root, &["font", "shell", "colors"], "", &mut diagnostics);

    let mut settings = Settings::default();
    parse_font(root.get("font"), &mut settings.font, &mut diagnostics);
    parse_shell(root.get("shell"), &mut settings.shell, &mut diagnostics);
    if let Some(colors) = root.get("colors") {
        match parse_colors(colors, &mut diagnostics) {
            Some(palette) => settings.colors = palette,
            None => diagnostics.push(error("colors: invalid palette; using all defaults")),
        }
    }

    SettingsLoad {
        settings,
        diagnostics,
    }
}

fn parse_font(
    value: Option<&Value>,
    settings: &mut FontSettings,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(value) = value else { return };
    let Some(table) = value.as_table() else {
        diagnostics.push(error("font must be a table; using font defaults"));
        return;
    };
    warn_unknown(table, &["family", "size"], "font", diagnostics);

    if let Some(value) = table.get("family") {
        match non_empty_string(value) {
            Some(family) => settings.family = Some(family.to_owned()),
            None => diagnostics.push(error(
                "font.family must be a non-empty string; using system selection",
            )),
        }
    }
    if let Some(value) = table.get("size") {
        let size = value
            .as_float()
            .or_else(|| value.as_integer().map(|value| value as f64));
        match size.filter(|size| size.is_finite() && (6.0..=144.0).contains(size)) {
            Some(size) => settings.size = size as f32,
            None => diagnostics.push(error(
                "font.size must be a finite number from 6 to 144; using 24",
            )),
        }
    }
}

fn parse_shell(
    value: Option<&Value>,
    settings: &mut ShellSettings,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(value) = value else { return };
    let Some(table) = value.as_table() else {
        diagnostics.push(error("shell must be a table; using shell defaults"));
        return;
    };
    warn_unknown(table, &["program", "args"], "shell", diagnostics);

    if let Some(value) = table.get("program") {
        match non_empty_string(value) {
            Some(program) => settings.program = Some(program.to_owned()),
            None => diagnostics.push(error(
                "shell.program must be a non-empty string; using platform fallback",
            )),
        }
    }
    if let Some(value) = table.get("args") {
        match value.as_array().and_then(|args| {
            args.iter()
                .map(Value::as_str)
                .map(|arg| arg.map(str::to_owned))
                .collect::<Option<Vec<_>>>()
        }) {
            Some(args) => settings.args = args,
            None => diagnostics.push(error(
                "shell.args must be an array of strings; using no arguments",
            )),
        }
    }
}

fn parse_colors(value: &Value, diagnostics: &mut Vec<Diagnostic>) -> Option<Palette> {
    let table = value.as_table()?;
    warn_unknown(
        table,
        &[
            "foreground",
            "background",
            "cursor",
            "selection",
            "normal",
            "bright",
        ],
        "colors",
        diagnostics,
    );
    let mut palette = Palette::default();

    parse_color_field(table, "foreground", &mut palette.foreground, diagnostics)?;
    parse_color_field(table, "background", &mut palette.background, diagnostics)?;
    parse_color_field(table, "cursor", &mut palette.cursor, diagnostics)?;
    parse_color_field(table, "selection", &mut palette.selection, diagnostics)?;
    parse_color_group(
        table.get("normal"),
        "colors.normal",
        &mut palette.normal,
        diagnostics,
    )?;
    parse_color_group(
        table.get("bright"),
        "colors.bright",
        &mut palette.bright,
        diagnostics,
    )?;
    Some(palette)
}

fn parse_color_field(
    table: &Table,
    key: &str,
    target: &mut Rgba,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<()> {
    let Some(value) = table.get(key) else {
        return Some(());
    };
    match parse_color(value) {
        Ok(color) => {
            *target = color;
            Some(())
        }
        Err(reason) => {
            diagnostics.push(error(format!("colors.{key} {reason}")));
            None
        }
    }
}

fn parse_color_group(
    value: Option<&Value>,
    path: &str,
    target: &mut [Rgba; 8],
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<()> {
    let Some(value) = value else { return Some(()) };
    let Some(table) = value.as_table() else {
        diagnostics.push(error(format!("{path} must be a table")));
        return None;
    };
    const NAMES: [&str; 8] = [
        "black", "red", "green", "yellow", "blue", "magenta", "cyan", "white",
    ];
    warn_unknown(table, &NAMES, path, diagnostics);
    for (index, name) in NAMES.iter().enumerate() {
        let Some(value) = table.get(*name) else {
            continue;
        };
        match parse_color(value) {
            Ok(color) => target[index] = color,
            Err(reason) => {
                diagnostics.push(error(format!("{path}.{name} {reason}")));
                return None;
            }
        }
    }
    Some(())
}

fn parse_color(value: &Value) -> Result<Rgba, &'static str> {
    value.as_str().ok_or("must be a string")?.parse::<Rgba>()
}

fn non_empty_string(value: &Value) -> Option<&str> {
    value
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn warn_unknown(table: &Table, known: &[&str], path: &str, diagnostics: &mut Vec<Diagnostic>) {
    for key in table.keys().filter(|key| !known.contains(&key.as_str())) {
        let name = if path.is_empty() {
            key.to_owned()
        } else {
            format!("{path}.{key}")
        };
        diagnostics.push(Diagnostic {
            level: DiagnosticLevel::Warning,
            message: format!("unknown setting `{name}` ignored"),
        });
    }
}

fn error(message: impl Into<String>) -> Diagnostic {
    Diagnostic {
        level: DiagnosticLevel::Error,
        message: message.into(),
    }
}

/// Unified compositor-level backdrop tint applied to the whole main window.
pub struct WindowBackdropStyle {
    pub tint_rgb: [f32; 3],
    pub tint_opacity: f32,
    pub luminosity_opacity: f32,
    pub fallback: [f32; 3],
}

impl Default for WindowBackdropStyle {
    fn default() -> Self {
        Self {
            tint_rgb: [1.0, 1.0, 1.0],
            tint_opacity: 0.06,
            luminosity_opacity: 1.0,
            fallback: [0.1176, 0.1176, 0.1176],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(source: &str) -> SettingsLoad {
        parse_document(toml::from_str(source).expect("test TOML must parse"))
    }

    #[test]
    fn valid_fields_override_defaults() {
        let loaded = parse(
            r##"
            [font]
            family = "Cascadia Mono"
            size = 18
            [shell]
            program = "pwsh.exe"
            args = ["-NoLogo"]
            [colors]
            background = "#5C331440"
            [colors.normal]
            red = "#CD3131"
        "##,
        );

        assert_eq!(
            loaded.settings.font.family.as_deref(),
            Some("Cascadia Mono")
        );
        assert_eq!(loaded.settings.font.size, 18.0);
        assert_eq!(loaded.settings.shell.program.as_deref(), Some("pwsh.exe"));
        assert_eq!(loaded.settings.shell.args, ["-NoLogo"]);
        assert_eq!(
            loaded.settings.colors.background,
            Rgba::from_rgba8(0x5c, 0x33, 0x14, 0x40)
        );
        assert_eq!(
            loaded.settings.colors.normal[1],
            Rgba::from_rgb8(0xcd, 0x31, 0x31)
        );
    }

    #[test]
    fn ordinary_invalid_fields_recover_independently() {
        let loaded = parse(
            r##"
            [font]
            family = "Cascadia Mono"
            size = -1
            [shell]
            program = "pwsh.exe"
            args = "-NoLogo"
        "##,
        );

        assert_eq!(
            loaded.settings.font.family.as_deref(),
            Some("Cascadia Mono")
        );
        assert_eq!(loaded.settings.font.size, FONT_SIZE);
        assert_eq!(loaded.settings.shell.program.as_deref(), Some("pwsh.exe"));
        assert!(loaded.settings.shell.args.is_empty());
        assert_eq!(loaded.diagnostics.len(), 2);
    }

    #[test]
    fn one_invalid_color_discards_all_color_overrides() {
        let loaded = parse(
            r##"
            [colors]
            foreground = "#123456"
            background = "not-a-color"
        "##,
        );
        assert_eq!(loaded.settings.colors, Palette::default());
    }
    #[test]
    fn non_ascii_invalid_color_discards_palette_without_panicking() {
        let loaded = parse(
            r##"
            [colors]
            foreground = "#aééx"
            background = "#123456"
        "##,
        );

        assert_eq!(loaded.settings.colors, Palette::default());
        assert!(loaded.diagnostics.iter().any(|diagnostic| {
            diagnostic.level == DiagnosticLevel::Error
                && diagnostic.message.contains("non-hexadecimal")
        }));
    }

    #[test]
    fn syntax_error_returns_complete_defaults() {
        let path = std::env::temp_dir().join(format!("harbor-invalid-{}.toml", std::process::id()));
        fs::write(&path, "[font\nsize = 12").unwrap();
        let loaded = load_from_path(&path);
        let _ = fs::remove_file(path);
        assert_eq!(loaded.settings, Settings::default());
        assert_eq!(loaded.diagnostics[0].level, DiagnosticLevel::Error);
    }

    #[test]
    fn missing_file_returns_complete_defaults_and_diagnostic() {
        let path = std::env::temp_dir().join(format!("harbor-missing-{}.toml", std::process::id()));
        let _ = fs::remove_file(&path);
        let loaded = load_from_path(path);
        assert_eq!(loaded.settings, Settings::default());
        assert_eq!(loaded.diagnostics.len(), 1);
    }

    #[test]
    fn unknown_keys_warn_without_discarding_known_values() {
        let loaded = parse("mystery = true\n[font]\nsize = 20\nweights = true");
        assert_eq!(loaded.settings.font.size, 20.0);
        assert_eq!(
            loaded
                .diagnostics
                .iter()
                .filter(|d| d.level == DiagnosticLevel::Warning)
                .count(),
            2
        );
    }

    #[test]
    fn should_expose_unified_backdrop_tint_defaults() {
        let style = WindowBackdropStyle::default();
        assert_eq!(style.tint_rgb, [1.0, 1.0, 1.0]);
        assert_eq!(style.tint_opacity, 0.06);
        assert_eq!(style.luminosity_opacity, 1.0);
        assert_eq!(style.fallback, [0.1176, 0.1176, 0.1176]);
    }
}

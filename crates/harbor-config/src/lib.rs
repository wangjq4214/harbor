//! Startup settings and application-wide behavior constants.
//!
//! User settings are loaded once from `~/.harbor/config.toml`. Missing or
//! unreadable files and invalid TOML fall back to complete defaults. Once TOML
//! parses, ordinary fields recover independently while the color palette is
//! validated and committed atomically.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

mod color;
pub use color::{Color, Palette, Rgba};
use toml::{Table, Value};

pub const FONT_SIZE: f32 = 24.0;
pub const DEFAULT_UI_FONT_SIZE: f32 = 12.0;
pub const TEXT_PADDING: f32 = 16.0;
/// Built-in terminal background (Catppuccin Mocha Base), not the window backdrop tint.
pub const BACKGROUND: [f32; 4] = [30.0 / 255.0, 30.0 / 255.0, 46.0 / 255.0, 0.85];
/// Built-in terminal selection highlight (Catppuccin Mocha Rosewater).
pub const SELECTION_COLOR: [f32; 4] = [245.0 / 255.0, 224.0 / 255.0, 220.0 / 255.0, 0.4];
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
    pub history: HistoryLimits,
    pub shell: ShellSettings,
    pub colors: Palette,
    pub keybindings: RawKeybindings,
    pub clipboard: ClipboardSettings,
}

/// Independent per-session primary-history bounds; bytes measure retained UTF-8 text,
/// not allocated memory. Zero is a zero allowance, never means unlimited.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HistoryLimits {
    pub max_rows: usize,
    pub max_bytes: usize,
}

impl HistoryLimits {
    pub const MAX_ROWS: usize = 16_000;
    pub const MAX_BYTES: usize = 16 * 1024 * 1024;

    pub fn is_valid(self) -> bool {
        self.max_rows <= Self::MAX_ROWS && self.max_bytes <= Self::MAX_BYTES
    }
}

impl Default for HistoryLimits {
    fn default() -> Self {
        Self {
            max_rows: 4_000,
            max_bytes: 4 * 1024 * 1024,
        }
    }
}
/// Startup policy for host-authorized OSC 52 system clipboard writes.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Osc52WritePolicy {
    #[default]
    Allow,
    Deny,
    Confirm,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ClipboardSettings {
    pub osc52_write: Osc52WritePolicy,
}

/// Raw structured bindings keyed by stable command ID. Command semantics are application-owned.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RawKeybindings {
    pub entries: BTreeMap<String, Vec<RawKeybinding>>,
    /// False when the TOML table or any command/binding had the wrong value shape.
    pub valid: bool,
}

/// One structured, single-stroke keybinding from startup configuration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RawKeybinding {
    pub modifiers: Vec<String>,
    pub key: String,
}

impl Default for RawKeybindings {
    fn default() -> Self {
        Self {
            entries: BTreeMap::new(),
            valid: true,
        }
    }
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

#[derive(Clone, Default, Eq, PartialEq)]
pub struct ShellSettings {
    /// Requested executable. `None` uses platform defaults (Windows: pwsh → powershell → COMSPEC/cmd).
    pub program: Option<String>,
    pub args: Vec<String>,
    /// Literal child-only overrides. Values are intentionally excluded from Debug.
    pub environment: BTreeMap<String, String>,
    /// Validated absolute directory, anchored at the configuration file.
    pub directory: Option<PathBuf>,
}

impl std::fmt::Debug for ShellSettings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ShellSettings")
            .field("program", &self.program)
            .field("args", &self.args)
            .field("environment", &"<redacted>")
            .field("directory", &self.directory)
            .finish()
    }
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

const PORTABLE_MARKER: &str = "harbor.portable";
const PORTABLE_CONFIG: &str = "config.toml";

fn config_path(
    executable: Option<&Path>,
    home_path: impl FnOnce() -> Option<PathBuf>,
) -> Option<PathBuf> {
    if let Some(directory) = executable.and_then(Path::parent)
        && directory.join(PORTABLE_MARKER).is_file()
    {
        return Some(directory.join(PORTABLE_CONFIG));
    }
    home_path()
}

/// Loads startup settings from the portable directory when marked, otherwise
/// from Harbor's default per-user path.
pub fn load() -> SettingsLoad {
    load_with_path().1
}

/// Resolves the configuration source once and returns it with startup settings.
/// Consumers must retain this path for watching, reloads and diagnostics.
pub fn load_with_path() -> (Option<PathBuf>, SettingsLoad) {
    let executable = std::env::current_exe().ok();
    let path = config_path(executable.as_deref(), default_config_path);
    let loaded = match path.as_ref() {
        Some(path) => load_from_path(path),
        None => SettingsLoad::defaults("could not resolve the user home directory"),
    };
    (path, loaded)
}

/// Loads startup settings from an injectable path.
pub fn load_from_path(path: impl AsRef<Path>) -> SettingsLoad {
    let path = path.as_ref();
    let source = match fs::read_to_string(path) {
        Ok(source) => source,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return SettingsLoad {
                settings: Settings::default(),
                diagnostics: Vec::new(),
            };
        }
        Err(error) => {
            return SettingsLoad::defaults(format!(
                "could not read settings {}: {error}",
                path.display()
            ));
        }
    };
    match toml::from_str::<Value>(&source) {
        Ok(document) => parse_document_at(document, path),
        Err(error) => SettingsLoad::defaults(format!(
            "could not parse settings {}: {}",
            path.display(),
            parse_error_location(&error, &source)
        )),
    }
}
/// A rejected live candidate. No fallback settings are exposed to the caller.
#[derive(Clone, Debug, PartialEq)]
pub struct CandidateError {
    pub path: PathBuf,
    /// Preserves absence separately from parse/validation/read failures.
    pub io_kind: Option<std::io::ErrorKind>,
    pub diagnostics: Vec<Diagnostic>,
}

impl std::fmt::Display for CandidateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: ", self.path.display())?;
        for (index, diagnostic) in self.diagnostics.iter().enumerate() {
            if index != 0 {
                write!(f, "; ")?;
            }
            write!(f, "{}", diagnostic.message)?;
        }
        Ok(())
    }
}

impl std::error::Error for CandidateError {}

/// Read a complete document for manual reload. Startup recovery remains separate.
/// Command/key semantics must additionally be validated by the application.
pub fn load_candidate_from_path(path: impl AsRef<Path>) -> Result<SettingsLoad, CandidateError> {
    let path = path.as_ref();
    let reject = |message: String| CandidateError {
        path: path.to_owned(),
        io_kind: None,
        diagnostics: vec![error(message)],
    };
    let source = fs::read_to_string(path).map_err(|err| CandidateError {
        io_kind: Some(err.kind()),
        ..reject(format!("could not read settings: {err}"))
    })?;
    let document = toml::from_str::<Value>(&source).map_err(|err| {
        reject(format!(
            "could not parse settings: {}",
            parse_error_location(&err, &source)
        ))
    })?;
    let parsed = parse_document_at(document, path);
    if parsed
        .diagnostics
        .iter()
        .any(|d| d.level == DiagnosticLevel::Error)
    {
        let diagnostics = parsed
            .diagnostics
            .into_iter()
            .map(|mut d| {
                // These shared validators describe startup fallback. Live rejection
                // must not suggest that fallback settings were installed.
                if d.level == DiagnosticLevel::Error {
                    d.message = d
                        .message
                        .split("; using")
                        .next()
                        .unwrap_or(&d.message)
                        .to_owned();
                }
                d
            })
            .collect();
        Err(CandidateError {
            path: path.to_owned(),
            io_kind: None,
            diagnostics,
        })
    } else {
        Ok(parsed)
    }
}

// Never format the TOML error: it contains source snippets and sometimes values.
fn parse_error_location(error: &toml::de::Error, source: &str) -> String {
    match error.span() {
        Some(span) => {
            let prefix = &source.as_bytes()[..span.start.min(source.len())];
            let line = prefix.iter().filter(|&&byte| byte == b'\n').count() + 1;
            let column = prefix
                .iter()
                .rposition(|&byte| byte == b'\n')
                .map_or(prefix.len() + 1, |last| prefix.len() - last);
            format!("invalid TOML at line {line}, column {column} (source omitted)")
        }
        None => "invalid TOML (source omitted)".to_owned(),
    }
}

fn parse_document_at(document: Value, path: &Path) -> SettingsLoad {
    let mut loaded = parse_document(document);
    if let Some(directory) = loaded.settings.shell.directory.take() {
        let resolved = (|| -> Result<PathBuf, &'static str> {
            if directory.as_os_str().is_empty() {
                return Err("must not be empty");
            }
            let base =
                std::path::absolute(path).map_err(|_| "could not resolve configuration path")?;
            let directory = if directory.is_absolute() {
                directory
            } else {
                base.parent()
                    .ok_or("configuration path has no parent")?
                    .join(directory)
            };
            // Never let a remaining Windows drive-relative path depend on a drive's cwd.
            if !directory.is_absolute() {
                return Err("must resolve to an absolute path");
            }
            let metadata = fs::metadata(&directory)
                .map_err(|_| "could not inspect directory (missing, invalid or inaccessible)")?;
            if !metadata.is_dir() {
                return Err("is not a directory");
            }
            Ok(directory)
        })();
        match resolved {
            Ok(directory) => loaded.settings.shell.directory = Some(directory),
            Err(reason) => loaded.diagnostics.push(error(format!(
                "shell.directory: {reason}; using inherited directory"
            ))),
        }
    }
    loaded
}

#[cfg(windows)]
fn environment_names_equal(a: &str, b: &str) -> bool {
    use windows::Win32::Globalization::{CSTR_EQUAL, CompareStringOrdinal};
    let a: Vec<_> = a.encode_utf16().collect();
    let b: Vec<_> = b.encode_utf16().collect();
    unsafe { CompareStringOrdinal(&a, &b, true) == CSTR_EQUAL }
}

#[cfg(not(windows))]
fn environment_names_equal(a: &str, b: &str) -> bool {
    a == b
}
fn parse_document(document: Value) -> SettingsLoad {
    let mut diagnostics = Vec::new();
    let Some(root) = document.as_table() else {
        return SettingsLoad::defaults("settings document must be a TOML table");
    };
    warn_unknown(
        root,
        &[
            "font",
            "shell",
            "colors",
            "keybindings",
            "clipboard",
            "history",
        ],
        "",
        &mut diagnostics,
    );

    let mut settings = Settings::default();
    parse_font(root.get("font"), &mut settings.font, &mut diagnostics);
    parse_history(root.get("history"), &mut settings.history, &mut diagnostics);
    parse_shell(root.get("shell"), &mut settings.shell, &mut diagnostics);
    parse_clipboard(
        root.get("clipboard"),
        &mut settings.clipboard,
        &mut diagnostics,
    );
    parse_keybindings(
        root.get("keybindings"),
        &mut settings.keybindings,
        &mut diagnostics,
    );
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

fn parse_history(
    value: Option<&Value>,
    settings: &mut HistoryLimits,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(value) = value else { return };
    let Some(table) = value.as_table() else {
        diagnostics.push(error("history must be a table; using history defaults"));
        return;
    };
    warn_unknown(table, &["max_rows", "max_bytes"], "history", diagnostics);
    for (key, target, maximum) in [
        ("max_rows", &mut settings.max_rows, HistoryLimits::MAX_ROWS),
        (
            "max_bytes",
            &mut settings.max_bytes,
            HistoryLimits::MAX_BYTES,
        ),
    ] {
        if let Some(value) = table.get(key) {
            match value
                .as_integer()
                .and_then(|v| usize::try_from(v).ok())
                .filter(|v| *v <= maximum)
            {
                Some(value) => *target = value,
                None => diagnostics.push(error(format!(
                    "history.{key} must be an integer from 0 to {maximum}; using default"
                ))),
            }
        }
    }
}
fn parse_clipboard(
    value: Option<&Value>,
    settings: &mut ClipboardSettings,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(value) = value else { return };
    let Some(table) = value.as_table() else {
        diagnostics.push(error("clipboard must be a table; using clipboard defaults"));
        return;
    };
    warn_unknown(table, &["osc52_write"], "clipboard", diagnostics);
    if let Some(value) = table.get("osc52_write") {
        match value.as_str() {
            Some("allow") => settings.osc52_write = Osc52WritePolicy::Allow,
            Some("deny") => settings.osc52_write = Osc52WritePolicy::Deny,
            Some("confirm") => settings.osc52_write = Osc52WritePolicy::Confirm,
            _ => diagnostics.push(error(
                "clipboard.osc52_write must be allow, deny, or confirm; using allow",
            )),
        }
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
    warn_unknown(
        table,
        &["program", "args", "environment", "directory"],
        "shell",
        diagnostics,
    );

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
    if let Some(value) = table.get("environment") {
        let parsed = (|| -> Result<BTreeMap<String, String>, &'static str> {
            let table = value.as_table().ok_or("must be a string-to-string table")?;
            let mut environment = BTreeMap::<String, String>::new();
            for (name, value) in table {
                let value = value.as_str().ok_or("values must be literal strings")?;
                if name.is_empty() || name.contains(['=', '\0']) {
                    return Err("names must be non-empty and contain neither '=' nor NUL");
                }
                if value.contains('\0') {
                    return Err("values must not contain NUL");
                }
                if environment
                    .keys()
                    .any(|key| environment_names_equal(key, name))
                {
                    return Err("contains conflicting case-insensitive names");
                }
                if environment_names_equal(name, "TERM") && value != "xterm-256color" {
                    return Err("TERM must remain xterm-256color");
                }
                environment.insert(name.clone(), value.to_owned());
            }
            Ok(environment)
        })();
        match parsed {
            Ok(environment) => settings.environment = environment,
            Err(reason) => diagnostics.push(error(format!(
                "shell.environment: {reason}; using inherited environment"
            ))),
        }
    }
    if let Some(value) = table.get("directory") {
        match value.as_str().filter(|path| !path.is_empty() && !path.contains('\0')) {
            Some(directory) => settings.directory = Some(PathBuf::from(directory)),
            None => diagnostics.push(error(
                "shell.directory must be a non-empty path string without NUL; using inherited directory",
            )),
        }
    }
}

fn parse_keybindings(
    value: Option<&Value>,
    settings: &mut RawKeybindings,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(value) = value else { return };
    let Some(namespaces) = value.as_table() else {
        invalidate_keybindings(
            settings,
            diagnostics,
            "keybindings must be a table of namespace tables",
        );
        return;
    };

    for (namespace, value) in namespaces {
        let path = format!("keybindings.{namespace}");
        let Some(commands) = value.as_table() else {
            invalidate_keybindings(
                settings,
                diagnostics,
                format!("{path} must be a table of command tables"),
            );
            continue;
        };

        for (command, value) in commands {
            let path = format!("{path}.{command}");
            let Some(command_table) = value.as_table() else {
                invalidate_keybindings(
                    settings,
                    diagnostics,
                    format!("{path} must be a table containing `bindings`"),
                );
                continue;
            };
            for field in command_table
                .keys()
                .filter(|field| field.as_str() != "bindings")
            {
                invalidate_keybindings(
                    settings,
                    diagnostics,
                    format!("{path}.{field} is not a supported keybinding field"),
                );
            }

            let Some(value) = command_table.get("bindings") else {
                invalidate_keybindings(
                    settings,
                    diagnostics,
                    format!("{path}.bindings is required"),
                );
                continue;
            };
            let Some(records) = value.as_array() else {
                invalidate_keybindings(
                    settings,
                    diagnostics,
                    format!("{path}.bindings must be an array of binding records"),
                );
                continue;
            };

            let mut bindings = Vec::with_capacity(records.len());
            for (index, record) in records.iter().enumerate() {
                if let Some(binding) =
                    parse_raw_keybinding(record, &path, index, settings, diagnostics)
                {
                    bindings.push(binding);
                }
            }
            settings
                .entries
                .insert(format!("{namespace}.{command}"), bindings);
        }
    }
}

fn parse_raw_keybinding(
    value: &Value,
    command_path: &str,
    index: usize,
    settings: &mut RawKeybindings,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<RawKeybinding> {
    let path = format!("{command_path}.bindings[{index}]");
    let Some(record) = value.as_table() else {
        invalidate_keybindings(
            settings,
            diagnostics,
            format!("{path} must be a table with `modifiers` and `key`"),
        );
        return None;
    };
    let mut structurally_valid = true;
    for field in record
        .keys()
        .filter(|field| !matches!(field.as_str(), "modifiers" | "key"))
    {
        invalidate_keybindings(
            settings,
            diagnostics,
            format!("{path}.{field} is not a supported binding field"),
        );
        structurally_valid = false;
    }

    let modifiers = match record.get("modifiers").and_then(Value::as_array) {
        Some(values) => match values
            .iter()
            .map(Value::as_str)
            .map(|value| value.map(str::to_owned))
            .collect::<Option<Vec<_>>>()
        {
            Some(modifiers) => modifiers,
            None => {
                invalidate_keybindings(
                    settings,
                    diagnostics,
                    format!("{path}.modifiers must be an array of strings"),
                );
                structurally_valid = false;
                Vec::new()
            }
        },
        None => {
            invalidate_keybindings(
                settings,
                diagnostics,
                format!("{path}.modifiers must be an array of strings"),
            );
            structurally_valid = false;
            Vec::new()
        }
    };
    let key = match record.get("key").and_then(Value::as_str) {
        Some(key) => key.to_owned(),
        None => {
            invalidate_keybindings(
                settings,
                diagnostics,
                format!("{path}.key must be a string"),
            );
            structurally_valid = false;
            String::new()
        }
    };

    structurally_valid.then_some(RawKeybinding { modifiers, key })
}

fn invalidate_keybindings(
    settings: &mut RawKeybindings,
    diagnostics: &mut Vec<Diagnostic>,
    message: impl Into<String>,
) {
    settings.valid = false;
    diagnostics.push(error(format!(
        "{}; using keybinding defaults",
        message.into()
    )));
}

fn parse_colors(value: &Value, diagnostics: &mut Vec<Diagnostic>) -> Option<Palette> {
    let table = value.as_table()?;
    warn_unknown(
        table,
        &[
            "foreground",
            "background",
            "background_opacity",
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
    // Fold the optional override into the palette so startup, OSC resets and
    // configuration reload all share one effective background alpha.
    if let Some(value) = table.get("background_opacity") {
        let opacity = value
            .as_float()
            .or_else(|| value.as_integer().map(|value| value as f64));
        let Some(opacity) =
            opacity.filter(|value| value.is_finite() && (0.0..=1.0).contains(value))
        else {
            diagnostics.push(error(
                "colors.background_opacity must be a finite number from 0 to 1",
            ));
            return None;
        };
        let [red, green, blue, _] = palette.background.components();
        palette.background = Rgba::new(red, green, blue, opacity as f32);
    }
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
mod tests;

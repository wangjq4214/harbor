//! Configurable terminal color and palette types.

// ── Color ────────────────────────────────────────────────────────────────────

/// Terminal color value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Color {
    /// Use the terminal's default foreground/background.
    Default,
    /// Standard ANSI colors 0-7.
    Named(u8),
    /// Bright ANSI colors 0-7 (rendered as palette entries 8-15).
    Bright(u8),
    /// 256-color palette index 0-255.
    Indexed(u8),
    /// Truecolor RGB.
    Rgb(u8, u8, u8),
}

/// Normalized red/green/blue/alpha color used at rendering boundaries.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rgba([f32; 4]);

impl Rgba {
    pub const fn new(red: f32, green: f32, blue: f32, alpha: f32) -> Self {
        Self([red, green, blue, alpha])
    }

    pub const fn from_rgb8(red: u8, green: u8, blue: u8) -> Self {
        Self::from_rgba8(red, green, blue, u8::MAX)
    }

    pub const fn from_rgba8(red: u8, green: u8, blue: u8, alpha: u8) -> Self {
        Self([
            red as f32 / 255.0,
            green as f32 / 255.0,
            blue as f32 / 255.0,
            alpha as f32 / 255.0,
        ])
    }

    pub const fn components(self) -> [f32; 4] {
        self.0
    }
}

impl From<Rgba> for [f32; 4] {
    fn from(value: Rgba) -> Self {
        value.components()
    }
}

impl std::str::FromStr for Rgba {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let hex = value.strip_prefix('#').ok_or("color must start with '#'")?;
        if !hex.is_ascii() {
            return Err("color contains a non-hexadecimal digit");
        }
        if hex.len() != 6 && hex.len() != 8 {
            return Err("color must contain 6 or 8 hexadecimal digits");
        }
        let byte = |offset| {
            u8::from_str_radix(&hex[offset..offset + 2], 16)
                .map_err(|_| "color contains a non-hexadecimal digit")
        };
        Ok(Self::from_rgba8(
            byte(0)?,
            byte(2)?,
            byte(4)?,
            if hex.len() == 8 { byte(6)? } else { u8::MAX },
        ))
    }
}

/// Runtime terminal palette. Protocol colors remain semantic [`Color`] values
/// and are resolved only when a renderer needs concrete RGBA components.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    pub foreground: Rgba,
    pub background: Rgba,
    pub cursor: Rgba,
    pub selection: Rgba,
    pub normal: [Rgba; 8],
    pub bright: [Rgba; 8],
}

impl Default for Palette {
    fn default() -> Self {
        Self {
            foreground: Rgba::from_rgb8(255, 255, 255),
            background: Rgba::new(0.36, 0.20, 0.08, 0.25),
            cursor: Rgba::from_rgba8(255, 255, 255, 204),
            selection: Rgba::new(0.3, 0.5, 0.9, 0.4),
            normal: [
                Rgba::from_rgb8(0, 0, 0),
                Rgba::from_rgb8(205, 0, 0),
                Rgba::from_rgb8(0, 205, 0),
                Rgba::from_rgb8(205, 205, 0),
                Rgba::from_rgb8(0, 0, 205),
                Rgba::from_rgb8(205, 0, 205),
                Rgba::from_rgb8(0, 205, 205),
                Rgba::from_rgb8(229, 229, 229),
            ],
            bright: [
                Rgba::from_rgb8(127, 127, 127),
                Rgba::from_rgb8(255, 0, 0),
                Rgba::from_rgb8(0, 255, 0),
                Rgba::from_rgb8(255, 255, 0),
                Rgba::from_rgb8(92, 92, 255),
                Rgba::from_rgb8(255, 0, 255),
                Rgba::from_rgb8(0, 255, 255),
                Rgba::from_rgb8(255, 255, 255),
            ],
        }
    }
}

impl Palette {
    /// Resolves a foreground/semantic color. [`Color::Default`] means the
    /// configured default foreground; default backgrounds are handled by the
    /// renderer's clear layer using [`Self::background`].
    pub fn resolve(&self, color: Color) -> [f32; 4] {
        match color {
            Color::Default => self.foreground.into(),
            Color::Named(n) => self
                .normal
                .get(n as usize)
                .copied()
                .unwrap_or(Rgba::from_rgb8(0, 0, 0))
                .into(),
            Color::Bright(n) => self
                .bright
                .get(n as usize)
                .copied()
                .unwrap_or(Rgba::from_rgb8(0, 0, 0))
                .into(),
            Color::Indexed(n @ 0..=7) => self.normal[n as usize].into(),
            Color::Indexed(n @ 8..=15) => self.bright[(n - 8) as usize].into(),
            Color::Indexed(n @ 16..=231) => {
                let index = n - 16;
                let expand = |component: u8| match component {
                    0 => 0.0,
                    1 => 95.0 / 255.0,
                    2 => 135.0 / 255.0,
                    3 => 175.0 / 255.0,
                    4 => 215.0 / 255.0,
                    _ => 1.0,
                };
                [
                    expand(index / 36),
                    expand((index % 36) / 6),
                    expand(index % 6),
                    1.0,
                ]
            }
            Color::Indexed(n) => {
                let value = (8 + (n - 232) * 10) as f32 / 255.0;
                [value, value, value, 1.0]
            }
            Color::Rgb(red, green, blue) => Rgba::from_rgb8(red, green, blue).into(),
        }
    }
}

impl Color {
    /// Resolves using the built-in default palette.
    ///
    /// New rendering code should prefer [`Palette::resolve`] with the palette
    /// supplied by application settings.
    pub fn to_rgba(self) -> [f32; 4] {
        Palette::default().resolve(self)
    }
}

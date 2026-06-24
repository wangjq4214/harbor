use std::fs;

use anyhow::{Context as _, Result};
use fontdb::{Database, Family, Query, Source, Stretch, Style, Weight};
use fontdue::Font;

/// Loads the default monospace font from the system font database.
pub(crate) fn load_system_font() -> Result<Font> {
    let mut fonts = Database::new();
    fonts.load_system_fonts();
    let face_id = fonts
        .query(&Query {
            families: &[Family::Monospace],
            weight: Weight::NORMAL,
            stretch: Stretch::Normal,
            style: Style::Normal,
        })
        .context("find system monospace font")?;
    let face = fonts.face(face_id).context("load system font face")?;
    // fontdb stores only the font source; normalize it to bytes for fontdue.
    let bytes = match &face.source {
        Source::File(path) => {
            fs::read(path).with_context(|| format!("read font {}", path.display()))?
        }
        Source::Binary(bytes) => bytes.as_ref().as_ref().to_vec(),
        Source::SharedFile(path, _) => {
            fs::read(path).with_context(|| format!("read font {}", path.display()))?
        }
    };

    Font::from_bytes(bytes, fontdue::FontSettings::default())
        .map_err(|error| anyhow::anyhow!("parse system font '{}': {error}", face.post_script_name))
}

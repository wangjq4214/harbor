//! Native-font geometry and retained GPU regression evidence for Spec 0044.
#[path = "nerd_icon_tests/native.rs"]
mod native;

use super::*;
use crate::model::SelectionBounds;
use crate::{Terminal, TerminalProjection, TerminalRenderPipeline, UpdateDamage};
use std::time::Instant;

const FAMILY: &str = "FiraMono Nerd Font";
const ICON: char = '\u{e5ff}'; // nf-custom-folder, Nerd Fonts v3.4.0 glyphnames.json

fn fonts(size: f32) -> FontBook {
    let fonts = harbor_text::load_system_fonts(&harbor_config::FontSettings {
        family: Some(FAMILY.into()),
        size,
    })
    .unwrap();
    assert!(
        fonts.selection_warning().is_none(),
        "installed {FAMILY} required: {:?}",
        fonts.selection_warning()
    );
    let harbor_text::GlyphResolution::Available(key) = fonts.resolve(ICON, size, 0) else {
        panic!("installed font must contain U+E5FF");
    };
    assert_eq!(
        key.face_id,
        harbor_text::FaceId::PRIMARY,
        "do not mistake fallback for configured font"
    );
    fonts
}

fn rect(verts: &[TexturedVertex], surface: (f32, f32)) -> [f32; 4] {
    let mut r = [
        f32::INFINITY,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NEG_INFINITY,
    ];
    for v in verts {
        let x = (v.position[0] + 1.0) * surface.0 * 0.5;
        let y = (1.0 - v.position[1]) * surface.1 * 0.5;
        r[0] = r[0].min(x);
        r[1] = r[1].min(y);
        r[2] = r[2].max(x);
        r[3] = r[3].max(y);
    }
    r
}

fn assert_geometry(layer: &Text, snap: &TerminalSnapshot, view: &RenderViewport) -> [f32; 4] {
    let glyph = layer.atlas.glyph_by_char(ICON).unwrap();
    let vertices = layer.build_range_vertices(
        &DirtyRange {
            row: 0,
            start_col: 0,
            end_col: 1,
        },
        snap,
        view,
    );
    let r = rect(&vertices, view.surface_dimensions());
    let (x, y) = view.cell_pos(0, 0);
    let left = glyph.bearing_x as f32;
    let top = layer.metrics.ascent.ceil() - glyph.bearing_y as f32 - glyph.height as f32;
    let expected = [
        x + left,
        y + top,
        x + left + glyph.width as f32,
        y + top + glyph.height as f32,
    ];
    for (actual, expected) in r.into_iter().zip(expected) {
        assert!(
            (actual - expected).abs() < 0.001,
            "natural overflow: {r:?}, expected {expected}"
        );
    }
    // Both native dimensions and the complete atlas UV must survive neighbor changes.
    let uv_width = vertices[2].tex_coords[0] - vertices[0].tex_coords[0];
    let uv_height = vertices[2].tex_coords[1] - vertices[0].tex_coords[1];
    assert!(
        (uv_width - (glyph.uv.right - glyph.uv.left) * (r[2] - r[0]) / glyph.width as f32).abs()
            < 0.00001
    );
    assert!(
        (uv_height - (glyph.uv.bottom - glyph.uv.top) * (r[3] - r[1]) / glyph.height as f32).abs()
            < 0.00001
    );
    assert_eq!(vertices[0].tex_coords, [glyph.uv.left, glyph.uv.top]);
    assert_eq!(vertices[2].tex_coords, [glyph.uv.right, glyph.uv.bottom]);
    r
}

fn fresh_pixels(
    gpu: TerminalGpuAccess<'_>,
    update: &TerminalProjection,
    view: &RenderViewport,
    size: f32,
) -> Vec<u8> {
    let f = fonts(size);
    let metrics = TextMetrics::from_font_metrics(f.font_metrics());
    let mut layer = Text::new(gpu, f, metrics, &update.snapshot, view, Palette::default()).unwrap();
    layer.prepare_with_dirty(gpu, &update.snapshot, &[], view, update.preedit.as_ref());
    layer.prepare_preedit(gpu, update.preedit.as_ref(), &update.snapshot, view);
    super::sequence_tests::read_draw(gpu.device(), gpu.queue(), |pass| layer.draw(pass))
}

/// Optional native readback artifact; never writes unless explicitly requested.
fn capture(name: &str, pixels: &[u8]) {
    let Some(dir) = std::env::var_os("HARBOR_ICON_EVIDENCE") else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    std::fs::create_dir_all(&dir).unwrap();
    // Portable PPM keeps test dependencies unchanged; RGB comes from actual GPU readback.
    let mut data = b"P6\n256 128\n255\n".to_vec();
    for pixel in pixels.as_chunks::<4>().0 {
        data.extend_from_slice(&pixel[..3]);
    }
    std::fs::write(dir.join(format!("{name}.ppm")), data).unwrap();
}

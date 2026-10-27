//! Native-font geometry and retained GPU regression evidence for Spec 0044.
use super::*;
use crate::model::SelectionBounds;
use crate::{Terminal, TerminalRenderPipeline, TerminalUpdate, UpdateDamage};
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
    update: &TerminalUpdate,
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

#[test]
#[ignore = "requires installed FiraMono Nerd Font and native Windows GPU; run explicitly"]
fn nerd_icons_native_overflow_and_retained_overlay_neighbor_updates() {
    let (device, queue) = super::sequence_tests::test_gpu().expect("native GPU required");
    let gpu = TerminalGpuAccess::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    let f = fonts(20.0);
    let metrics = TextMetrics::from_font_metrics(f.font_metrics());
    let mut engine = Terminal::new_headless(3, 12);
    engine.put_str(&format!("{ICON}   X"));
    let initial = engine.read_update(Instant::now());
    let mut renderer = TerminalRenderPipeline::new(gpu, (256, 128), f, metrics, &initial).unwrap();
    assert!(renderer.prepare(gpu, &initial, true));
    assert!(engine.acknowledge_update(&initial));
    let view = renderer.viewport();
    let glyph = renderer.text.atlas.glyph_by_char(ICON).unwrap();
    let (native_width, native_height) = (glyph.width as f32, glyph.height as f32);
    let native_left = glyph.bearing_x as f32;
    let native_top = metrics.ascent.ceil() - glyph.bearing_y as f32 - native_height;
    eprintln!(
        "Spec0044 font={FAMILY}, size=20px, icon=nf-custom-folder U+E5FF, collection=v3.4.0; cell={}x{}, ink={}x{}, bearings={},{}; Windows DirectWrite -> R8 atlas -> wgpu 256x128 Rgba8Unorm",
        metrics.cell_width,
        metrics.line_height,
        glyph.width,
        glyph.height,
        glyph.bearing_x,
        glyph.bearing_y
    );
    assert!(
        glyph.width as f32 > metrics.cell_width,
        "fixture must expose squeezing"
    );
    assert!(
        glyph.width as f32 <= 4.0 * metrics.cell_width
            && glyph.height as f32 <= metrics.line_height,
        "fixture must expose natural-size retention"
    );
    assert_geometry(&renderer.text, &initial.snapshot, &view);
    let natural =
        super::sequence_tests::read_draw(&device, &queue, |pass| renderer.text.draw(pass));
    capture("natural", &natural);
    assert_eq!(natural, fresh_pixels(gpu, &initial, &view, 20.0));
    let copy_bounds = SelectionBounds {
        start_row: 0,
        start_col: 0,
        end_row: 0,
        end_col: 4,
    };
    let copied = engine.screen.selected_text(copy_bounds);
    assert_eq!(copied, format!("{ICON}   X"));
    assert_eq!(initial.snapshot.cursor_x, 5);

    // All overlay-only transitions have empty PTY damage; source coordinates/data are unchanged.
    for kind in ["cursor", "selection", "preedit"] {
        let mut update = initial.clone();
        update.damage = UpdateDamage::Ranges(vec![]);
        match kind {
            "cursor" => std::sync::Arc::make_mut(&mut update.snapshot).cursor_x = 1,
            "selection" => {
                update.selection = Some(SelectionBounds {
                    start_row: 0,
                    start_col: 1,
                    end_row: 0,
                    end_col: 1,
                })
            }
            _ => {
                std::sync::Arc::make_mut(&mut update.snapshot).cursor_x = 1;
                std::sync::Arc::make_mut(&mut update.snapshot).cursor_visible = false;
                update.preedit = Some(Preedit::new("中", None));
            }
        }
        let cells = update.snapshot.cells.clone();
        assert!(renderer.prepare(gpu, &update, true));
        assert_geometry(&renderer.text, &update.snapshot, &view);
        let blocked =
            super::sequence_tests::read_draw(&device, &queue, |pass| renderer.text.draw(pass));
        capture(kind, &blocked);
        // Even when preedit composites above the overhang, source-cell ink
        // and the complete icon geometry remain unchanged.
        let (icon_x, icon_y) = view.cell_pos(0, 0);
        for y in icon_y as usize..(icon_y + view.line_height) as usize {
            let start = (y * 256 + icon_x as usize) * 4;
            let end = start + view.cell_width as usize * 4;
            assert_eq!(
                &blocked[start..end],
                &natural[start..end],
                "{kind} kept ink"
            );
        }
        if kind == "preedit" {
            // Existing IME compositing remains above terminal glyphs.
            assert_ne!(blocked, natural);
        } else {
            assert_eq!(blocked, natural, "{kind} must not crop the icon");
        }
        assert_eq!(blocked, fresh_pixels(gpu, &update, &view, 20.0), "{kind}");
        assert_eq!(update.snapshot.cells, cells);
        let mut restored = initial.clone();
        restored.damage = UpdateDamage::Ranges(vec![]);
        assert!(renderer.prepare(gpu, &restored, false));
        assert_eq!(
            super::sequence_tests::read_draw(&device, &queue, |pass| renderer.text.draw(pass)),
            natural,
            "{kind} removal"
        );
    }

    assert_eq!(engine.screen.selected_text(copy_bounds), copied);
    assert_eq!(engine.snapshot().wrapped, initial.snapshot.wrapped);

    // Text and narrowed cell geometry do not crop or scale the native quad.
    for (block_col, cell_width) in [(1, view.cell_width), (2, view.cell_width), (2, 7.0)] {
        let mut update = initial.clone();
        std::sync::Arc::make_mut(&mut update.snapshot).cells[block_col].ch = 'Z';
        update.damage = UpdateDamage::FullUpload;
        let mut constrained = view;
        constrained.cell_width = cell_width;
        assert!(renderer.sync_viewport(constrained, false));
        assert!(renderer.prepare(gpu, &update, true));
        assert_geometry(&renderer.text, &update.snapshot, &constrained);
        let actual =
            super::sequence_tests::read_draw(&device, &queue, |pass| renderer.text.draw(pass));
        assert_eq!(actual, fresh_pixels(gpu, &update, &constrained, 20.0));
        capture(&format!("text-{block_col}-width-{cell_width}"), &actual);
    }
    assert!(renderer.sync_viewport(view, false));
    let mut edge = initial.clone();
    std::sync::Arc::make_mut(&mut edge.snapshot).cells[0] = Cell::default();
    std::sync::Arc::make_mut(&mut edge.snapshot).cells[11] = initial.snapshot.cells[0].clone();
    edge.damage = UpdateDamage::FullUpload;
    assert!(renderer.prepare(gpu, &edge, true));
    let quad = renderer.text.build_range_vertices(
        &DirtyRange {
            row: 0,
            start_col: 11,
            end_col: 12,
        },
        &edge.snapshot,
        &view,
    );
    let r = rect(&quad, view.surface_dimensions());
    let (x, y) = view.cell_pos(0, 11);
    assert!((r[2] - r[0] - native_width).abs() < 0.001);
    assert!((r[3] - r[1] - native_height).abs() < 0.001);
    assert!((r[0] - x - native_left).abs() < 0.001);
    assert!((r[1] - y - native_top).abs() < 0.001);
    assert!(
        r[2] > x + view.cell_width,
        "row edge must not crop the quad"
    );
    assert_eq!(
        super::sequence_tests::read_draw(&device, &queue, |pass| renderer.text.draw(pass)),
        fresh_pixels(gpu, &edge, &view, 20.0)
    );

    // Synthetic edge placement above was not an engine mutation. Restore a
    // complete engine snapshot before applying genuine neighbor-only damage.
    let mut reset = initial.clone();
    reset.damage = UpdateDamage::FullUpload;
    assert!(renderer.prepare(gpu, &reset, true));
    assert_eq!(
        super::sequence_tests::read_draw(&device, &queue, |pass| renderer.text.draw(pass)),
        natural
    );

    // Neighbor-only writes: source icon is NOT in the dirty range.
    for stream in [
        "\x1b[1;2HZ",
        "\x1b[1;2H\x1b[41m \x1b[0m",
        "\x1b[1;2H\x1b[4m \x1b[0m",
        "\x1b[1;2H\x1b]8;;https://example.test\x1b\\ \x1b]8;;\x1b\\",
        "\x1b[1;2H\x1b[1\"q \x1b[0\"q",
        "\x1b[1;2H\x1b[8ma\x1b[0m",
        "\x1b[1;2H \u{301}",
    ] {
        engine.put_str(stream);
        engine.put_str("\x1b[3;1H"); // don't accidentally test cursor obstruction
        let update = engine.read_update(Instant::now());
        let UpdateDamage::Ranges(ranges) = &update.damage else {
            panic!("incremental expected")
        };
        assert!(
            ranges.iter().all(|r| r.row != 0 || r.start_col > 0),
            "{ranges:?}"
        );
        assert!(renderer.prepare(gpu, &update, true));
        assert_geometry(&renderer.text, &update.snapshot, &view);
        assert_eq!(
            super::sequence_tests::read_draw(&device, &queue, |pass| renderer.text.draw(pass)),
            fresh_pixels(gpu, &update, &view, 20.0),
            "{stream:?}"
        );
        assert!(engine.acknowledge_update(&update));
        engine.put_str("\x1b[1;2H\x1b[0m \x1b[3;1H");
        let restored = engine.read_update(Instant::now());
        assert!(renderer.prepare(gpu, &restored, true));
        assert_eq!(
            super::sequence_tests::read_draw(&device, &queue, |pass| renderer.text.draw(pass)),
            natural
        );
        assert!(engine.acknowledge_update(&restored));
    }

    // Source styling must not reinstate the old background-dependent restriction.
    for inverse in [false, true] {
        let mut styled = initial.clone();
        styled.damage = UpdateDamage::FullUpload;
        std::sync::Arc::make_mut(&mut styled.snapshot).cells[0].bg = Color::Named(1);
        if inverse {
            std::sync::Arc::make_mut(&mut styled.snapshot).cells[0]
                .attrs
                .set(CellAttrs::INVERSE);
        }
        assert!(renderer.prepare(gpu, &styled, true));
        assert_geometry(&renderer.text, &styled.snapshot, &view);
        assert_eq!(
            super::sequence_tests::read_draw(&device, &queue, |pass| renderer.text.draw(pass)),
            fresh_pixels(gpu, &styled, &view, 20.0)
        );
    }
    assert!(renderer.prepare(gpu, &reset, true));

    // Full artwork really paints past the source cell, but still obeys the
    // outer scissor supplied by the host. This is not per-cell clipping.
    let empty = super::sequence_tests::read_draw(&device, &queue, |_| {});
    let clipped = super::sequence_tests::read_draw(&device, &queue, |pass| {
        pass.set_scissor_rect(16, 16, 12, 24);
        renderer.text.draw(pass);
    });
    let mut overflow_pixels = 0;
    for y in 0..128 {
        for x in 0..256 {
            let i = (y * 256 + x) * 4;
            let inside = (16..28).contains(&x) && (16..40).contains(&y);
            let expected = if inside {
                &natural[i..i + 4]
            } else {
                &empty[i..i + 4]
            };
            assert_eq!(&clipped[i..i + 4], expected, "scissor pixel {x},{y}");
            if (28..35).contains(&x)
                && (21..36).contains(&y)
                && natural[i..i + 4] != empty[i..i + 4]
            {
                overflow_pixels += 1;
            }
        }
    }
    assert!(overflow_pixels > 0, "native artwork must visibly overflow");
    capture("outer-scissor", &clipped);
    capture(
        "full-pipeline",
        &super::sequence_tests::read_draw(&device, &queue, |pass| renderer.draw(pass)),
    );

    // Real source-cell erasure/movement must clear the old overhang even when
    // the renderer only uploads the engine's incremental cell ranges.
    let mut moving = Terminal::new_headless(3, 12);
    moving.put_str(&format!("{ICON}   X"));
    let first = moving.read_update(Instant::now());
    let f = fonts(20.0);
    let mut retained = TerminalRenderPipeline::new(gpu, (256, 128), f, metrics, &first).unwrap();
    assert!(retained.prepare(gpu, &first, true));
    assert!(moving.acknowledge_update(&first));
    for stream in [
        format!("\x1b[1;1H \x1b[2;3H{ICON}"),
        "\x1b[2;3H \x1b[3;1H".to_owned(),
        format!("\x1b[1;1H{ICON}"),
    ] {
        moving.put_str(&stream);
        let next = moving.read_update(Instant::now());
        assert!(matches!(next.damage, UpdateDamage::Ranges(_)));
        assert!(retained.prepare(gpu, &next, true));
        assert_eq!(
            super::sequence_tests::read_draw(&device, &queue, |pass| retained.text.draw(pass)),
            fresh_pixels(gpu, &next, &retained.viewport(), 20.0)
        );
        assert!(moving.acknowledge_update(&next));
    }

    let mut update = initial.clone();
    update.damage = UpdateDamage::Ranges(vec![]);
    assert!(renderer.sync_raster_scale(gpu, 1.5, &update).unwrap());
    assert_geometry(&renderer.text, &update.snapshot, &view);
    let mut shifted = view;
    shifted.allocation_origin = (3.25, 2.5);
    shifted.cell_width *= 0.75;
    shifted.line_height = 8.0; // deliberately shorter than native ink
    assert!(renderer.sync_viewport(shifted, false));
    assert!(renderer.prepare(gpu, &update, true));
    assert_geometry(&renderer.text, &update.snapshot, &shifted);
    assert_eq!(
        super::sequence_tests::read_draw(&device, &queue, |pass| renderer.text.draw(pass)),
        fresh_pixels(gpu, &update, &shifted, 20.0)
    );
    renderer.replace_fonts(gpu, fonts(16.0), &update).unwrap();
    assert!(renderer.prepare(gpu, &update, true));
    assert_geometry(&renderer.text, &update.snapshot, &renderer.viewport());
    assert_eq!(update.snapshot.cells, initial.snapshot.cells);
    assert_eq!(engine.screen.selected_text(copy_bounds), copied);
    assert_eq!(update.snapshot.cell(0, 0).grid_width(), 1);
    assert_eq!(update.snapshot.cell(0, 0).raw_text(), ICON.to_string());
}

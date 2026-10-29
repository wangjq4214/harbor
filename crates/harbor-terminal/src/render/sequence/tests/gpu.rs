use super::*;
#[path = "incremental.rs"]
mod incremental;

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn gpu_color_upload_is_untinted_linear_premultiplied_and_cell_bounded() {
    let (device, queue) = crate::render::text::sequence_tests::test_gpu();
    let gpu = TerminalGpuAccess::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    let request = SequenceRequest::new(
        "😀",
        FontSize::new(16.0).unwrap(),
        FontSize::new(96.0).unwrap(),
    );
    let generation = harbor_text::PresentationGeneration {
        session: 1,
        revision: 0,
    };
    for kind in [
        harbor_text::CompleteKind::Color,
        harbor_text::CompleteKind::Monochrome,
    ] {
        let mut atlas = ColorAtlas::default();
        atlas.sync(std::slice::from_ref(&request), generation, |_| {
            harbor_text::SequencePresentation {
                generation,
                outcome: harbor_text::SequenceOutcome::Complete {
                    kind,
                    tile: harbor_text::PresentationTile {
                        bounds: TileBounds {
                            left: -40,
                            top: -80,
                            width: 512,
                            height: 512,
                        },
                        image_formats: 0,
                        rgba: [128, 0, 0, 128].repeat(512 * 512),
                    },
                    runs: vec![],
                },
            }
        });
        let entry = &atlas.entries[&request];
        let p = entry.placement.unwrap();
        let tile = entry.tile.as_ref().unwrap();
        let side = COLOR_ATLAS_SIDE as f32;
        // Green per-vertex tint must NOT alter a baked red tile, even for a
        // monochrome tile whose current foreground was already rasterized.
        let quad = fitted_quad(
            tile.bounds,
            [
                p.x as f32 / side,
                p.y as f32 / side,
                (p.x + 512) as f32 / side,
                (p.y + 512) as f32 / side,
            ],
            [20.0, 30.0, 18.0, 19.0],
            (256.0, 128.0),
            [0.0, 1.0, 0.0, 1.0],
        )
        .unwrap();
        let color = ColorGpu::new(gpu);
        color.upload_full(gpu, &atlas);
        let mut quads = VertexBuffer::new(&device);
        quads.write(gpu, &quad);
        let pixels = crate::render::text::sequence_tests::read_draw(&device, &queue, |pass| {
            quads.draw(pass, &color.pipeline, Some(&color.bind_group))
        });
        let sample = &pixels[(40 * 256 + 30) * 4..(40 * 256 + 30) * 4 + 4];
        assert!(
            sample[0].abs_diff(128) <= 1
                && sample[1] == 0
                && sample[2].abs_diff(32) <= 1
                && sample[3] == 255,
            "partial alpha contract: {sample:?}"
        );
        for y in 0..128 {
            for x in 0..256 {
                if !(20..38).contains(&x) || !(30..49).contains(&y) {
                    assert_eq!(
                        &pixels[(y * 256 + x) * 4..(y * 256 + x) * 4 + 4],
                        &[0, 0, 64, 255],
                        "neighbor changed at {x},{y}"
                    );
                }
            }
        }
    }
}

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn unsupported_missing_cue_is_visible_bounded_and_not_complete_color() {
    let (device, queue) = crate::render::text::sequence_tests::test_gpu();
    let gpu = TerminalGpuAccess::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    let fonts = harbor_text::load_system_fonts(&harbor_config::FontSettings::default()).unwrap();
    let mut engine = crate::Terminal::new_headless(2, 8);
    engine.put_str("👩");
    let mut snap = (*engine.read_update(std::time::Instant::now()).snapshot).clone();
    snap.cells[0].suffix = "\u{200d}\u{301}".repeat(128);
    let source = snap.cells[0].raw_text();
    let viewport = RenderViewport::with_surface(9.0, 19.0, (256, 128), (256, 128));
    let mut layer = SequenceLayer::default();
    layer.prepare(
        gpu,
        &fonts,
        &GlyphAtlas::new(),
        &snap,
        &viewport,
        &Palette::default(),
        1.0,
    );
    assert!(layer.atlas.pixels.is_empty());
    assert!(layer.tile_cells.contains(&0));
    assert_eq!(snap.cells[0].raw_text(), source);
    assert_eq!(snap.cells[0].grid_width(), 2);
    let pixels = crate::render::text::sequence_tests::read_draw(&device, &queue, |p| layer.draw(p));
    let clear = crate::render::text::sequence_tests::read_draw(&device, &queue, |_| {});
    assert_ne!(pixels, clear, "missing glyph cannot be invisible");
    for (i, (actual, expected)) in pixels
        .as_chunks::<4>()
        .0
        .iter()
        .zip(clear.as_chunks::<4>().0.iter())
        .enumerate()
    {
        let (x, y) = (i % 256, i / 256);
        if !(16..34).contains(&x) || !(16..35).contains(&y) {
            assert_eq!(actual, expected);
        }
    }
}

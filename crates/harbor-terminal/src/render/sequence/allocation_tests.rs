use super::*;
use crate::primary_reflow::allocation_probe::measure;
use crate::render::text::sequence_tests::test_gpu;

#[test]
#[ignore = "Windows native fonts and GPU; run explicitly"]
fn hot_path_sequence_allocation_measurements() {
    let (device, queue) = test_gpu();
    let gpu = TerminalGpuAccess::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    let fonts = harbor_text::load_system_fonts(&harbor_config::FontSettings {
        family: Some("Consolas".into()),
        size: 16.0,
    })
    .unwrap();
    let palette = Palette::default();
    let scalar = GlyphAtlas::new();
    for (rows, cols) in [(24, 80), (80, 240)] {
        let terminal = crate::Terminal::new_headless_with_history(
            rows,
            cols,
            crate::TerminalAppearance::default(),
            harbor_config::HistoryLimits {
                max_rows: 0,
                max_bytes: 0,
            },
        );
        let mut snap = terminal.snapshot();
        snap.cells[0].ch = '1';
        snap.cells[0].suffix = "\u{fe0f}\u{20e3}".into();
        snap.cells[0].width = 2;
        snap.cells[cols].ch = '🚀';
        snap.cells[cols].width = 2;
        let viewport = RenderViewport::with_surface(
            9.0,
            19.0,
            ((cols * 9) as u32, (rows * 19) as u32),
            ((cols * 9) as u32, (rows * 19) as u32),
        );
        let dirty = [crate::DirtyRange {
            row: 0,
            start_col: 0,
            end_col: cols,
        }];
        let mut layer = SequenceLayer::default();
        let (_, cold) = measure(|| {
            layer.prepare(gpu, &fonts, &scalar, &snap, &viewport, &palette, 1.0);
        });
        eprintln!("HOT sequence layer_initial {rows}x{cols} {cold:?}");
        for width in [1, 2, 1, 2] {
            snap.cells[0].width = width;
            layer.prepare_with_dirty(
                gpu, &fonts, &scalar, &snap, &viewport, &palette, 1.0, &dirty, false, false,
            );
        }
        let (_, replay) = measure(|| {
            for _ in 0..64 {
                std::hint::black_box(layer.prepare_with_dirty(
                    gpu, &fonts, &scalar, &snap, &viewport, &palette, 1.0, &dirty, false, false,
                ));
            }
        });
        let (_, local) = measure(|| {
            for i in 0..64 {
                snap.cells[0].width = if i % 2 == 0 { 1 } else { 2 };
                std::hint::black_box(layer.prepare_with_dirty(
                    gpu, &fonts, &scalar, &snap, &viewport, &palette, 1.0, &dirty, false, false,
                ));
            }
        });
        assert_eq!(
            crate::render::observations::sequence_work().built_rows,
            vec![0]
        );
        eprintln!("HOT sequence {rows}x{cols} n=64 replay={replay:?} local_width={local:?}");

        assert_eq!(
            replay.calls, 0,
            "stable replay reuses flags, candidates and requests"
        );
        let (_, dependencies) = measure(|| {
            for _ in 0..64 {
                layer.prepare_with_dirty(
                    gpu,
                    &fonts,
                    &scalar,
                    &snap,
                    &viewport,
                    &palette,
                    1.0,
                    &[],
                    false,
                    true,
                );
            }
        });
        assert_eq!(
            dependencies.calls, 0,
            "atlas/suppression/geometry scratch is reusable"
        );
        eprintln!("HOT sequence stable_dependencies {rows}x{cols} n=64 {dependencies:?}");
        assert!(layer.scratch.touched.capacity() <= rows * 2);
        assert!(layer.scratch.cells.is_empty());
        assert!(layer.scratch.cells.capacity() <= cols * 2);
        assert!(layer.scratch.vertices.is_empty());
        assert!(layer.scratch.vertices.capacity() <= cols * 12);
        assert!(layer.scratch.missing.is_empty());
        assert!(layer.scratch.missing.capacity() <= cols * 48);
        assert!(layer.scratch.usability.capacity() <= rows * cols * 2);
    }
}

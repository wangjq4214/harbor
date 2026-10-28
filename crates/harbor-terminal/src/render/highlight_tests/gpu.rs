use super::*;

#[test]
#[ignore = "gpu: run the matching explicit test recipe"]
fn gpu_highlights_use_current_geometry_cache_and_remove_stale_text_without_cell_mutation() {
    let (device, queue) = crate::test_gpu::device();
    let gpu = TerminalGpuAccess::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    let pipeline = Arc::new(crate::render::gpu::create_colored_quad_pipeline(
        &device,
        gpu.format(),
        "highlight test",
    ));
    let mut layer = Highlights::new(gpu, pipeline);
    let mut t = Terminal::new_headless(2, 8);
    t.put_str("ab界");
    let version = t.content_version();
    let mut gate = LiteralSearch::default();
    let mut job = gate
        .start(
            &t,
            "界",
            ReadScope::Retained {
                start_row: 0,
                row_count: 2,
            },
            ReadBudget::default(),
        )
        .unwrap();
    while job.step(&t) == SearchStatus::Running {}
    let results = gate.accept(&t, job.into_results()).unwrap();
    let spans = t.project_content_ranges(results.ranges());
    let mut viewport = RenderViewport::with_surface(8.0, 8.0, (64, 32), (64, 32));
    viewport.padding = 0.0;
    layer.prepare(gpu, &spans, viewport);
    assert_eq!(layer.quads.count(), 6);
    let pixels = raster(&device, &queue, &layer);
    let pixel = |p: &[u8], x: usize, y: usize| p[(y * 64 + x) * 4..(y * 64 + x + 1) * 4].to_vec();
    assert!(pixel(&pixels, 17, 3)[0] > 0);
    assert_eq!(pixel(&pixels, 15, 3), [0, 0, 0, 0]);
    assert_eq!(pixel(&pixels, 32, 3), [0, 0, 0, 0]);
    layer.prepare(gpu, &spans, viewport);
    assert_eq!(
        raster(&device, &queue, &layer),
        pixels,
        "cached frame is identical"
    );
    viewport.allocation_origin = (8.0, 8.0);
    layer.prepare(gpu, &spans, viewport);
    let moved = raster(&device, &queue, &layer);
    assert_eq!(pixel(&moved, 17, 3), [0, 0, 0, 0]);
    assert!(pixel(&moved, 25, 11)[0] > 0);
    assert_eq!(
        t.content_version(),
        version,
        "painting doesn't mutate terminal cells"
    );
    t.put_str("\rX");
    layer.prepare(gpu, &t.project_content_ranges(results.ranges()), viewport);
    assert_eq!(layer.quads.count(), 0);
    assert!(raster(&device, &queue, &layer).iter().all(|b| *b == 0));
}

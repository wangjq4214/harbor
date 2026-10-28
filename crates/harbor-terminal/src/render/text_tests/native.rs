use super::*;

#[test]
#[ignore = "requires supporting Windows Segoe UI Emoji/native color capabilities and GPU; run explicitly"]
fn fragmented_heart_and_zwj_replace_old_pixels_and_converge_after_erase() {
    let (device, queue) = test_gpu();
    let gpu = TerminalGpuAccess::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    for chunks in [vec!["♥", "\u{fe0f}"], vec!["👩", "\u{200d}", "💻"]] {
        let mut engine = Terminal::new_headless(4, 16);
        engine.process_output(b"\x1b[1;4HN\x1b[1;1H");
        let initial = engine.read_update(Instant::now());
        let (mut layer, viewport) = text(gpu, &initial.snapshot);
        engine.acknowledge_update(&initial);
        let mut previous = read_draw(&device, &queue, |p| layer.draw(p));
        for chunk in chunks {
            engine.process_output(chunk.as_bytes());
            let update = engine.read_update(Instant::now());
            assert!(
                update
                    .snapshot
                    .dirty_ranges
                    .iter()
                    .any(|r| r.row == 0 && r.start_col == 0 && r.end_col >= 1)
            );
            assert_eq!(
                engine.read_update(Instant::now()).snapshot.cells,
                update.snapshot.cells,
                "skipped draw retains source"
            );
            layer.prepare_with_dirty(
                gpu,
                &update.snapshot,
                &update.snapshot.dirty_ranges,
                &viewport,
                None,
            );
            layer.prepare_preedit(gpu, None, &update.snapshot, &viewport);
            let actual = read_draw(&device, &queue, |p| layer.draw(p));
            let (mut fresh, _) = text(gpu, &update.snapshot);
            fresh.prepare_preedit(gpu, None, &update.snapshot, &viewport);
            let full = read_draw(&device, &queue, |p| fresh.draw(p));
            assert_eq!(actual, full, "incremental/full converge for {chunk:?}");
            assert_eq!(update.snapshot.cell(0, 3).ch, 'N');
            if update.snapshot.cell(0, 0).raw_text() == "♥️"
                || update.snapshot.cell(0, 0).raw_text() == "👩‍💻"
            {
                assert!(
                    layer.sequences.tile_cells.contains(&0),
                    "named installed Windows fixture must produce a complete tile"
                );
                assert!(layer.sequences.atlas.entries.values().any(|e| e.outcome
                    == crate::render::color_atlas::CachedOutcome::Complete(
                        harbor_text::CompleteKind::Color
                    )));
                assert_ne!(actual, previous, "complete source replaces prior artwork");
                let request = layer
                    .sequences
                    .atlas
                    .entries
                    .keys()
                    .find(|r| r.source == update.snapshot.cell(0, 0).raw_text())
                    .unwrap();
                eprintln!(
                    "complete request {}: {:?}",
                    request.source, layer.sequences.atlas.entries[request].outcome
                );
                let calls = layer.fonts.sequence_cache_stats();
                layer.prepare_with_dirty(
                    gpu,
                    &update.snapshot,
                    &update.snapshot.dirty_ranges,
                    &viewport,
                    None,
                );
                assert_eq!(
                    layer.fonts.sequence_cache_stats().shape_calls,
                    calls.shape_calls
                );
                assert_eq!(
                    layer.fonts.sequence_cache_stats().raster_calls,
                    calls.raster_calls
                );
            }
            previous = actual;
            engine.acknowledge_update(&update);
        }
        engine.process_output(b"\x1b[1;1H\x1b[2K");
        let erase = engine.read_update(Instant::now());
        layer.prepare_with_dirty(
            gpu,
            &erase.snapshot,
            &erase.snapshot.dirty_ranges,
            &viewport,
            None,
        );
        layer.prepare_preedit(gpu, None, &erase.snapshot, &viewport);
        let (mut fresh, _) = text(gpu, &erase.snapshot);
        fresh.prepare_preedit(gpu, None, &erase.snapshot, &viewport);
        assert_eq!(
            read_draw(&device, &queue, |p| layer.draw(p)),
            read_draw(&device, &queue, |p| fresh.draw(p))
        );
        assert!(layer.sequences.tile_cells.is_empty());
    }
}

#[test]
#[ignore = "requires supporting Windows Segoe UI Emoji/native color capabilities and GPU; run explicitly"]
fn complete_tiles_suppress_combining_overlays_and_conceal_removes_presentation() {
    let (device, queue) = test_gpu();
    let gpu = TerminalGpuAccess::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    let mut engine = Terminal::new_headless(4, 16);
    engine.put_str("♥️");
    let mut snap = (*engine.read_update(Instant::now()).snapshot).clone();
    let (mut layer, viewport) = text(gpu, &snap);
    assert!(layer.sequences.tile_cells.contains(&0));
    // A mark on a tile-represented cell must not be painted a second time.
    snap.cells[0].suffix.push('\u{301}');
    layer.prepare_preedit(gpu, None, &snap, &viewport);
    assert_eq!(layer.overlay.count(), 0);
    snap.cells[0].attrs.set(CellAttrs::CONCEAL);
    layer.invalidate_projection();
    layer.prepare_with_dirty(gpu, &snap, &[], &viewport, None);
    layer.prepare_preedit(gpu, None, &snap, &viewport);
    assert!(layer.sequences.tile_cells.is_empty());
    assert_eq!(layer.overlay.count(), 0);
    assert_eq!(
        read_draw(&device, &queue, |p| layer.draw(p)),
        read_draw(&device, &queue, |_| {})
    );
}

#[test]
#[ignore = "requires supporting Windows Segoe UI Emoji/native color capabilities and GPU; run explicitly"]
fn preedit_paints_above_complete_tiles_at_an_overlapping_cursor() {
    let (device, queue) = test_gpu();
    let gpu = TerminalGpuAccess::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    let mut engine = Terminal::new_headless(4, 16);
    engine.put_str("♥️");
    engine.process_output(b"\x1b[1;1H");
    let snap = engine.read_update(Instant::now()).snapshot;
    let (mut layer, viewport) = text(gpu, &snap);
    let before = read_draw(&device, &queue, |p| layer.draw(p));
    let preedit = Preedit::new("M", None);
    layer.prepare_with_dirty(gpu, &snap, &[], &viewport, Some(&preedit));
    layer.prepare_preedit(gpu, Some(&preedit), &snap, &viewport);
    assert!(layer.overlay.count() > 0);
    let actual = read_draw(&device, &queue, |p| layer.draw(p));
    // An explicit scalar overlay drawn after a sequence-only frame is the reference.
    let expected = read_draw(&device, &queue, |pass| {
        layer.sequences.draw(pass);
        pass.set_pipeline(&layer.pipeline);
        pass.set_bind_group(0, &layer.gpu_atlas.bind_group, &[]);
        pass.set_vertex_buffer(0, layer.overlay.buffer().slice(..));
        pass.draw(0..layer.overlay.count(), 0..1);
    });
    assert_eq!(actual, expected);
    assert_ne!(actual, before);
}

use super::*;

fn quad() -> [ColoredVertex; 6] {
    ColoredVertex::from_pixel_rect(0.0, 0.0, 1.0, 1.0, [1.0; 4], 100.0, 100.0)
}

#[test]
fn compaction_removes_only_zero_area_padding_and_keeps_triangle_order() {
    let mut padded = vec![ColoredVertex::default(); 12];
    padded.extend_from_slice(&quad());
    let thin = [
        ColoredVertex {
            position: [0.0, 0.0],
            color: [1.0; 4],
        },
        ColoredVertex {
            position: [f32::MIN_POSITIVE, 0.0],
            color: [1.0; 4],
        },
        ColoredVertex {
            position: [0.0, f32::MIN_POSITIVE],
            color: [1.0; 4],
        },
    ];
    padded.extend_from_slice(&thin);
    let mut compact = Vec::new();
    ColoredRows::append(&mut compact, &padded);
    assert_eq!(compact.len(), 9);
    assert_eq!(
        bytemuck::cast_slice::<_, u8>(&compact[..6]),
        bytemuck::cast_slice::<_, u8>(&quad())
    );
    assert_eq!(
        bytemuck::cast_slice::<_, u8>(&compact[6..]),
        bytemuck::cast_slice::<_, u8>(&thin)
    );
}

mod gpu {
    use super::*;

    fn gpu<'a>(device: &'a wgpu::Device, queue: &'a wgpu::Queue) -> TerminalGpuAccess<'a> {
        TerminalGpuAccess::new(device, queue, wgpu::TextureFormat::Rgba8Unorm)
    }

    fn fill(vertices: &mut Vec<ColoredVertex>, count: usize) {
        vertices.extend(std::iter::repeat_n(quad()[0], count));
    }

    fn dirty(row: usize) -> DirtyRange {
        DirtyRange {
            row,
            start_col: 0,
            end_col: 1,
        }
    }

    #[test]
    #[ignore = "gpu: run the matching explicit test recipe"]
    fn capacity_threshold_is_strict_and_empty_storage_is_released() {
        let (device, queue) = crate::test_gpu::device();
        let gpu = gpu(&device, &queue);
        let mut rows = ColoredRows::default();
        rows.prepare(gpu, 1, &[], true, |_, v| fill(v, 48));
        // Deterministic CPU capacity, independent of Vec's growth heuristic.
        let mut cpu = Vec::with_capacity(96);
        cpu.extend_from_slice(&rows.rows[0].vertices);
        rows.rows[0].vertices = cpu;
        let buffer = rows.buffers();
        rows.prepare(gpu, 1, &[dirty(0)], false, |_, v| fill(v, 24));
        assert_eq!(rows.vertex_storage(), (24, 96, 96));
        assert_eq!(rows.buffers(), buffer, "exactly 4x does not force shrink");
        rows.prepare(gpu, 1, &[dirty(0)], false, |_, v| fill(v, 21));
        assert_eq!(rows.vertex_storage(), (21, 42, 21));
        let bytes = crate::render::observations::buffer_write_bytes(|| {
            rows.prepare(gpu, 1, &[], false, |_, _| panic!("warm rebuild"));
        });
        assert_eq!(bytes, 0);
        rows.prepare(gpu, 1, &[dirty(0)], false, |_, _| {});
        assert_eq!(rows.storage_bytes(), (0, 0, 0));
        rows.prepare(gpu, 200, &[], true, |_, _| {});
        assert_eq!(rows.storage_bytes(), (0, 0, 0), "no per-row placeholders");
        eprintln!(
            "empty rows=200 geometry_bytes={:?} metadata_bytes={}",
            rows.storage_bytes(),
            rows.rows.capacity() * std::mem::size_of::<Row>()
        );
    }

    #[test]
    #[ignore = "gpu: run the matching explicit test recipe"]
    fn aggregate_trigger_trims_untouched_rows_and_local_damage_stays_local() {
        let (device, queue) = crate::test_gpu::device();
        let gpu = gpu(&device, &queue);
        let mut rows = ColoredRows::default();
        rows.prepare(gpu, 2, &[], true, |r, v| fill(v, [24, 48][r]));
        for (row, capacity) in rows.rows.iter_mut().zip([48, 96]) {
            let mut cpu = Vec::with_capacity(capacity);
            cpu.extend_from_slice(&row.vertices);
            row.vertices = cpu;
        }
        let untouched = rows.buffers()[1].clone();
        let mut built = Vec::new();
        let bytes = crate::render::observations::buffer_write_bytes(|| {
            rows.prepare(gpu, 2, &[dirty(0), dirty(0)], false, |r, v| {
                built.push(r);
                fill(v, 12);
            });
        });
        assert_eq!(built, [0], "overlapping damage builds each row once");
        assert_eq!(bytes, 12 * std::mem::size_of::<ColoredVertex>());
        assert_eq!(rows.buffers()[1], untouched);
        assert_eq!(rows.vertex_storage(), (60, 144, 144));
        built.clear();
        rows.prepare(gpu, 2, &[dirty(1)], false, |r, v| {
            built.push(r);
            fill(v, 12);
        });
        assert_eq!(
            built,
            [1],
            "reclamation need not rebuild untouched geometry"
        );
        let (d, g, c) = rows.vertex_storage();
        assert_eq!(d, 24);
        assert!(g <= 2 * d && c <= 2 * d, "aggregate 2x target: {g}/{c}/{d}");
        // A row-count reduction must count capacity that is dropped by truncation.
        rows.prepare(gpu, 1, &[], true, |_, v| fill(v, 3));
        let (d, g, c) = rows.vertex_storage();
        assert_eq!(d, 3);
        assert!(g <= 2 * d && c <= 2 * d);
        rows.prepare(gpu, 2, &[], true, |r, v| fill(v, [6, 9][r]));
        assert_eq!(rows.vertex_storage().0, 15);
    }

    #[test]
    #[ignore = "gpu: run the matching explicit test recipe"]
    fn growth_spare_capacity_cannot_break_the_layer_bound() {
        let (device, queue) = crate::test_gpu::device();
        let gpu = gpu(&device, &queue);
        let mut rows = ColoredRows::default();
        rows.prepare(gpu, 2, &[], true, |r, v| fill(v, [6, 30][r]));
        rows.prepare(gpu, 2, &[dirty(0), dirty(1)], false, |r, v| {
            fill(v, [12, 6][r])
        });
        assert_eq!(rows.vertex_storage().0, 18);
        assert_eq!(rows.vertex_storage().1, 72, "exactly four times demand");
        // Growing row 0 from 12 to 15 needs a new 30-vertex allocation.
        // Checking only the old 72 misses the resulting 90 > 4 * 21.
        rows.prepare(gpu, 2, &[dirty(0)], false, |_, v| fill(v, 15));
        let (d, g, c) = rows.vertex_storage();
        assert_eq!(d, 21);
        assert!(
            g <= 4 * d && c <= 4 * d,
            "growth exceeded bound: {d}/{g}/{c}"
        );
    }
}

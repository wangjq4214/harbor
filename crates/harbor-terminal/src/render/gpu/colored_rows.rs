//! Sparse colored geometry retained by row, used only by boxes and decorations.
//! Empty rows hold no vertex allocation (M = 0). Row descriptors are metadata;
//! all heap-resident vertex payload is accounted for below.
use super::{ColoredVertex, TerminalGpuAccess, create_sized_vertex_buffer};
use crate::DirtyRange;

#[derive(Default)]
struct Row {
    vertices: Vec<ColoredVertex>,
    buffer: Option<wgpu::Buffer>,
    changed: bool,
}

#[derive(Default)]
pub(crate) struct ColoredRows {
    rows: Vec<Row>,
}

impl ColoredRows {
    /// Call only after validating the grid, ranges and projection. Global
    /// invalidation rebuilds all rows; ordinary damage rebuilds touched rows.
    pub(crate) fn prepare(
        &mut self,
        gpu: TerminalGpuAccess<'_>,
        row_count: usize,
        ranges: &[DirtyRange],
        full: bool,
        mut build: impl FnMut(usize, &mut Vec<ColoredVertex>),
    ) {
        if !full && ranges.is_empty() {
            return;
        }
        let (_, old_gpu, old_cpu) = self.vertex_storage();
        self.rows.resize_with(row_count, Row::default);
        // Descriptor capacity is not vertex staging, but don't retain its peak either.
        if self.rows.capacity() > row_count.saturating_mul(4) {
            self.rows = std::mem::take(&mut self.rows).into_boxed_slice().into_vec();
        }
        for row in &mut self.rows {
            row.changed = full;
        }
        if !full {
            for range in ranges.iter().filter(|r| r.start_col < r.end_col) {
                self.rows[range.row].changed = true;
            }
        }
        for (index, row) in self.rows.iter_mut().enumerate() {
            if row.changed {
                row.vertices.clear();
                build(index, &mut row.vertices);
            }
        }

        let (demand, _, cpu_capacity) = self.vertex_storage();
        let max_vertices = (gpu.device().limits().max_buffer_size
            / std::mem::size_of::<ColoredVertex>() as u64) as usize;
        let projected_gpu: usize = self
            .rows
            .iter()
            .map(|row| {
                let count = row.vertices.len();
                let capacity = buffer_vertices(row.buffer.as_ref());
                if count > capacity {
                    count.saturating_mul(2).min(max_vertices)
                } else {
                    capacity
                }
            })
            .sum();
        // Include both dropped storage and upcoming growth reserves. Testing
        // rows independently would not establish the layer-wide 2x target.
        let trim_gpu = old_gpu.max(projected_gpu) > demand.saturating_mul(4);
        let trim_cpu = old_cpu.max(cpu_capacity) > demand.saturating_mul(4);
        for row in &mut self.rows {
            let count = row.vertices.len();
            if count == 0 {
                *row = Row::default();
                continue;
            }
            if trim_cpu && row.vertices.capacity() > count.saturating_mul(2) {
                row.vertices = std::mem::take(&mut row.vertices)
                    .into_boxed_slice()
                    .into_vec();
            }
            let capacity = buffer_vertices(row.buffer.as_ref());
            if count > capacity || (trim_gpu && capacity > count.saturating_mul(2)) {
                row.buffer = Some(create_sized_vertex_buffer::<ColoredVertex>(
                    gpu.device(),
                    count.saturating_mul(2).min(max_vertices),
                ));
                row.changed = true;
            }
            if row.changed {
                gpu.write_buffer(
                    row.buffer.as_ref().expect("nonempty row has storage"),
                    0,
                    bytemuck::cast_slice(&row.vertices),
                );
                row.changed = false;
            }
        }
    }

    /// Discard only exactly zero-area padding, never thin visible triangles.
    /// f64 arithmetic avoids overflow/underflow while testing f32 positions.
    pub(crate) fn append(vertices: &mut Vec<ColoredVertex>, cell: &[ColoredVertex]) {
        for triangle in cell.as_chunks::<3>().0 {
            let [a, b, c] = triangle.map(|v| v.position.map(f64::from));
            let area = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
            if area != 0.0 {
                vertices.extend_from_slice(triangle);
            }
        }
    }

    pub(crate) fn draw(&self, pass: &mut wgpu::RenderPass, pipeline: &wgpu::RenderPipeline) {
        pass.set_pipeline(pipeline);
        for row in &self.rows {
            if let Some(buffer) = &row.buffer {
                pass.set_vertex_buffer(0, buffer.slice(..));
                pass.draw(0..row.vertices.len() as u32, 0..1);
            }
        }
    }

    /// Demand / held GPU / held CPU capacity, in vertices. No stored counters
    /// can conceal allocations: inspect the actual buffer sizes and Vec capacities.
    fn vertex_storage(&self) -> (usize, usize, usize) {
        self.rows.iter().fold((0, 0, 0), |(d, g, c), row| {
            (
                d + row.vertices.len(),
                g + buffer_vertices(row.buffer.as_ref()),
                c + row.vertices.capacity(),
            )
        })
    }

    #[cfg(test)]
    pub(crate) fn storage_bytes(&self) -> (usize, usize, usize) {
        let (d, g, c) = self.vertex_storage();
        let size = std::mem::size_of::<ColoredVertex>();
        (d * size, g * size, c * size)
    }

    #[cfg(test)]
    pub(crate) fn buffers(&self) -> Vec<Option<wgpu::Buffer>> {
        self.rows.iter().map(|r| r.buffer.clone()).collect()
    }
}

fn buffer_vertices(buffer: Option<&wgpu::Buffer>) -> usize {
    buffer.map_or(0, |b| {
        b.size() as usize / std::mem::size_of::<ColoredVertex>()
    })
}

#[cfg(test)]
mod tests;

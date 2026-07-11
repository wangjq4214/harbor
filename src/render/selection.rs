use crate::{
    config::{SELECTION_COLOR, TEXT_PADDING},
    render::{
        Component, EventResult, SelectionInput,
        caps::{ModifiersAccess, PtyAccess, RedrawAccess, TerminalAccess},
        gpu::{self, ColoredVertex, GpuContext},
    },
    terminal::{Screen, SelectionBounds, Terminal},
};
use arboard::Clipboard;

// ── Selection shader ────────────────────────────────────────────────────────

/// Simple untextured shader that renders per-vertex color quads (identical to
/// Decoration's shader, duplicated per "no shared GPU objects" convention).
const SELECTION_SHADER: &str = r#"
struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) color: vec4<f32>,
}
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
}
@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.position = vec4<f32>(in.position, 0.0, 1.0);
    out.color = in.color;
    return out;
}
@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return in.color;
}
"#;

// ── Selection model ─────────────────────────────────────────────────────────

/// Tracks the current text selection as a pair of grid coordinates.
/// `anchor` is where the drag started; `cursor` is the current drag endpoint.
#[derive(Clone, Copy, Debug)]
struct SelectionRange {
    anchor: (usize, usize), // (row, col)
    cursor: (usize, usize), // (row, col)
}

impl SelectionRange {
    /// Returns `(start_row, start_col, end_row, end_col)` in row-major reading order.
    /// Guarantees start ≤ end in row-major.
    fn normalized(&self) -> (usize, usize, usize, usize) {
        if self.anchor.0 < self.cursor.0
            || (self.anchor.0 == self.cursor.0 && self.anchor.1 <= self.cursor.1)
        {
            (self.anchor.0, self.anchor.1, self.cursor.0, self.cursor.1)
        } else {
            (self.cursor.0, self.cursor.1, self.anchor.0, self.anchor.1)
        }
    }
}

// ── Selection ────────────────────────────────────────────────

pub(crate) struct Selection {
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    /// Number of vertices to draw (0 when no selection).
    vertex_count: u32,
    /// Current vertex buffer capacity (rows * cols * 6).
    vertex_cap: usize,
    /// None = no active selection.
    selection: Option<SelectionRange>,
    /// True while left mouse button is held.
    dragging: bool,
    /// Cached from the most recent CursorMoved event (physical pixels).
    /// Needed because winit 0.30 MouseInput does not carry a position.
    last_cursor_pos: Option<(f64, f64)>,
    cell_width: f32,
    line_height: f32,
    /// Whether vertex buffer needs re-upload.
    dirty: bool,
    /// System clipboard handle (None when clipboard is unavailable, e.g. headless).
    clipboard: Option<Clipboard>,
}

impl Selection {
    pub(crate) fn new(gpu: &GpuContext, cell_width: f32, line_height: f32) -> Self {
        let pipeline = Self::create_pipeline(gpu.device(), gpu.format());
        let vertex_buffer = gpu::create_colored_vertex_buffer(gpu.device(), &[]);
        Self {
            pipeline,
            vertex_buffer,
            vertex_count: 0,
            vertex_cap: 0,
            selection: None,
            dragging: false,
            last_cursor_pos: None,
            cell_width,
            line_height,
            dirty: false,
            clipboard: {
                let cb = Clipboard::new();
                if cb.is_err() {
                    tracing::warn!("clipboard unavailable; copy/paste will be disabled");
                }
                cb.ok()
            },
        }
    }

    /// Compiles the selection shader into a render pipeline.
    fn create_pipeline(device: &wgpu::Device, format: wgpu::TextureFormat) -> wgpu::RenderPipeline {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("selection shader"),
            source: wgpu::ShaderSource::Wgsl(SELECTION_SHADER.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("selection pipeline layout"),
            bind_group_layouts: &[],
            immediate_size: 0,
        });

        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("selection pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[Some(ColoredVertex::layout())],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        })
    }

    /// Convert a physical-pixel cursor position to a grid cell (row, col).
    /// Clamps to grid bounds; never returns None.
    fn pixel_to_cell(&self, x: f64, y: f64, rows: usize, cols: usize) -> (usize, usize) {
        let col_f = ((x as f32 - TEXT_PADDING) / self.cell_width).floor();
        let row_f = ((y as f32 - TEXT_PADDING) / self.line_height).floor();
        let col = col_f.clamp(0.0, cols.saturating_sub(1) as f32) as usize;
        let row = row_f.clamp(0.0, rows.saturating_sub(1) as f32) as usize;
        (row, col)
    }

    /// Grow the vertex buffer if the current capacity is too small for the grid.
    fn ensure_capacity(&mut self, gpu: &GpuContext, rows: usize, cols: usize) {
        let needed = rows * cols * 6;
        if needed > self.vertex_cap {
            self.vertex_buffer = gpu::create_colored_vertex_buffer(
                gpu.device(),
                &vec![ColoredVertex::default(); needed],
            );
            self.vertex_cap = needed;
        }
    }

    /// Build ColoredVertex quads for every cell in the current selection.
    /// One quad = 6 vertices per selected cell.
    fn build_vertices(&self, screen: &Screen, surf_w: f32, surf_h: f32) -> Vec<ColoredVertex> {
        let sel = self.selection.unwrap();
        let (sr, sc, er, ec) = sel.normalized();
        let cols = screen.cols();
        let mut verts = Vec::new();

        for row in sr..=er {
            let col_start = if row == sr { sc } else { 0 };
            let col_end = if row == er {
                ec
            } else {
                cols.saturating_sub(1)
            };

            for col in col_start..=col_end {
                let left = TEXT_PADDING + col as f32 * self.cell_width;
                let top = TEXT_PADDING + row as f32 * self.line_height;
                let right = left + self.cell_width;
                let bottom = top + self.line_height;

                let quad = ColoredVertex::from_pixel_rect(
                    left,
                    top,
                    right,
                    bottom,
                    SELECTION_COLOR,
                    surf_w,
                    surf_h,
                );
                verts.extend_from_slice(&quad);
            }
        }
        verts
    }

    /// Returns the currently selected text, or `None` when there is no active selection.
    fn selected_text(&self, screen: &Screen) -> Option<String> {
        let sel = self.selection?;
        let (start_row, start_col, end_row, end_col) = sel.normalized();
        let text = screen.selected_text(SelectionBounds {
            start_row,
            start_col,
            end_row,
            end_col,
        });
        if text.is_empty() { None } else { Some(text) }
    }

    /// Intercepts Ctrl+C (copy selection) and Ctrl+V (paste). Returns
    /// `None` when the event is not a keyboard shortcut we handle.
    fn try_handle_keyboard<C>(
        &mut self,
        event: &winit::event::WindowEvent,
        caps: &mut C,
    ) -> Option<EventResult>
    where
        C: TerminalAccess + PtyAccess + ModifiersAccess,
    {
        let winit::event::WindowEvent::KeyboardInput { event: kbd, .. } = event else {
            return None;
        };
        if kbd.state != winit::event::ElementState::Pressed || !caps.modifiers().control_key() {
            return None;
        }

        match &kbd.logical_key {
            winit::keyboard::Key::Character(ch) if ch == "c" || ch == "C" => {
                let Some(text) = self.selected_text(caps.terminal().screen()) else {
                    return Some(EventResult::Continue);
                };
                if let Some(clipboard) = self.clipboard.as_mut()
                    && let Err(e) = clipboard.set_text(text)
                {
                    tracing::warn!(error = %e, "failed to set clipboard text");
                }
                Some(EventResult::Handled)
            }
            winit::keyboard::Key::Character(ch) if ch == "v" || ch == "V" => {
                if let Some(clipboard) = self.clipboard.as_mut() {
                    match clipboard.get_text() {
                        Ok(text) => caps.pty().write(text.as_bytes()),
                        Err(e) => tracing::warn!(error = %e, "failed to read clipboard text"),
                    }
                }
                // Always Handled — never send \x16 to the PTY.
                Some(EventResult::Handled)
            }
            _ => None,
        }
    }

    fn handle_cursor_moved(
        &mut self,
        position: winit::dpi::PhysicalPosition<f64>,
        terminal: &Terminal,
        redraw: &impl RedrawAccess,
    ) -> EventResult {
        self.last_cursor_pos = Some((position.x, position.y));

        if !self.dragging {
            return EventResult::Continue;
        }

        let screen = terminal.screen();
        let (row, col) = self.pixel_to_cell(position.x, position.y, screen.rows(), screen.cols());
        if let Some(sel) = &mut self.selection
            && sel.cursor != (row, col)
        {
            sel.cursor = (row, col);
            self.dirty = true;
            redraw.request_redraw();
        }
        EventResult::Handled
    }

    fn handle_mouse_input(
        &mut self,
        state: winit::event::ElementState,
        button: winit::event::MouseButton,
        terminal: &Terminal,
        redraw: &impl RedrawAccess,
    ) -> EventResult {
        if button != winit::event::MouseButton::Left {
            return EventResult::Continue;
        }

        match state {
            winit::event::ElementState::Pressed => {
                if let Some((x, y)) = self.last_cursor_pos {
                    let screen = terminal.screen();
                    let (row, col) = self.pixel_to_cell(x, y, screen.rows(), screen.cols());
                    self.selection = Some(SelectionRange {
                        anchor: (row, col),
                        cursor: (row, col),
                    });
                    self.dragging = true;
                    self.dirty = true;
                    redraw.request_redraw();
                }
                EventResult::Handled
            }
            winit::event::ElementState::Released => {
                if self.dragging {
                    self.dragging = false;
                    // Click without drag → clear selection.
                    if let Some(sel) = self.selection
                        && sel.anchor == sel.cursor
                    {
                        self.selection = None;
                        self.dirty = true;
                        redraw.request_redraw();
                    }
                }
                EventResult::Handled
            }
        }
    }
}

impl SelectionInput for Selection {
    fn handle_event<C>(&mut self, event: &winit::event::WindowEvent, caps: &mut C) -> EventResult
    where
        C: TerminalAccess + RedrawAccess + PtyAccess + ModifiersAccess,
    {
        // Ctrl+C/V clipboard — before alt-screen check so paste works in
        // vim/less and copy works from scrollback.
        if let Some(result) = self.try_handle_keyboard(event, caps) {
            return result;
        }

        // In alt-screen mode, let the terminal application handle all mouse events.
        // Cancel any in-flight drag so state doesn't leak past the boundary.
        if caps.terminal().is_alt_screen() {
            self.dragging = false;
            return EventResult::Continue;
        }

        match event {
            winit::event::WindowEvent::CursorMoved { position, .. } => {
                self.handle_cursor_moved(*position, caps.terminal(), caps)
            }
            winit::event::WindowEvent::MouseInput { state, button, .. } => {
                self.handle_mouse_input(*state, *button, caps.terminal(), caps)
            }
            _ => EventResult::Continue,
        }
    }
}

impl Component for Selection {
    fn prepare(&mut self, gpu: &GpuContext, screen: Option<&Screen>) {
        if !self.dirty {
            return;
        }
        self.dirty = false;

        let Some(screen) = screen else {
            self.vertex_count = 0;
            return;
        };

        if let Some(_sel) = self.selection {
            let rows = screen.rows();
            let cols = screen.cols();
            self.ensure_capacity(gpu, rows, cols);

            let (surf_w, surf_h) = gpu.surface_size();
            let verts = self.build_vertices(screen, surf_w as f32, surf_h as f32);
            gpu.queue()
                .write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&verts));
            self.vertex_count = verts.len() as u32;
        } else {
            self.vertex_count = 0;
        }
    }

    fn draw(&self, pass: &mut wgpu::RenderPass) {
        if self.vertex_count == 0 {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.draw(0..self.vertex_count, 0..1);
    }

    fn resize(&mut self, _gpu: &GpuContext, _size: (u32, u32)) {
        // Grid dimensions changed; old selection coordinates are stale.
        self.selection = None;
        self.dirty = true;
    }
}

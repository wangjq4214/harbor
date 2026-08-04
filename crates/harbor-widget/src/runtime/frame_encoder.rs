//! GPU frame encoding: renderer ownership and paint-order draw batching.

use crate::renderer::Viewport;
use crate::renderer::quad::QuadRenderer;
use crate::renderer::text_renderer::TextRenderer;
use crate::scene::primitive::{ExternalDrawContext, ExternalDrawFn, ExternalDrawId};
use crate::scene::{SceneDelta, SceneGraph};
use crate::text::TextRunCache;
use crate::widgets::text_label;
use hashbrown::HashMap;
use std::sync::Arc;

/// Owns GPU renderers and encodes a paint-ordered scene into a render pass.
///
/// Lifecycle: created empty with the Runtime; renderers are initialized once a
/// wgpu Device is available; encode runs each frame after the tree paint pass.
pub(crate) struct FrameEncoder {
    renderer: Option<QuadRenderer>,
    text_renderer: Option<TextRenderer>,
    text_run_cache: TextRunCache,
}

impl FrameEncoder {
    pub(crate) fn new() -> Self {
        Self {
            renderer: None,
            text_renderer: None,
            text_run_cache: TextRunCache::new(),
        }
    }

    pub(crate) fn init_renderer(&mut self, device: &wgpu::Device, format: wgpu::TextureFormat) {
        self.renderer = Some(QuadRenderer::new(device, format));
    }

    pub(crate) fn init_text_renderer(
        &mut self,
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        bind_group_layout: &wgpu::BindGroupLayout,
        bind_group: &wgpu::BindGroup,
    ) {
        self.text_renderer = Some(TextRenderer::new(
            device,
            format,
            bind_group_layout,
            bind_group,
        ));
    }

    pub(crate) fn text_run_cache(&mut self) -> &mut TextRunCache {
        &mut self.text_run_cache
    }

    /// Drains thread-local pending text runs and registers them with the cache.
    pub(crate) fn register_pending_text_runs(&mut self, glyph_fn: &crate::text::GlyphFn<'_>) {
        let pending = text_label::drain_pending_text_runs();
        if pending.is_empty() {
            return;
        }

        let metrics = crate::text::current_metrics().unwrap_or(crate::text::TextMetrics {
            cell_width: 10.0,
            line_height: 20.0,
            ascent: 16.0,
            underline_position: 0.0,
            underline_thickness: 1.5,
            strikethrough_position: 0.0,
            strikethrough_thickness: 1.5,
        });

        for (id, text, _color) in &pending {
            self.text_run_cache
                .register_with_id(*id, text, &metrics, glyph_fn);
        }
    }

    /// Applies a pending SceneDelta and encodes draw calls in paint order.
    pub(crate) fn encode<'a>(
        &'a mut self,
        queue: &wgpu::Queue,
        pass: &mut wgpu::RenderPass<'a>,
        viewport: Viewport,
        scene_graph: &SceneGraph,
        pending_delta: &mut Option<SceneDelta>,
        current_viewport: &mut Option<Viewport>,
        external_draws: &HashMap<ExternalDrawId, Arc<ExternalDrawFn<'static>>>,
    ) {
        let renderer = match self.renderer.as_mut() {
            Some(r) => r,
            None => return,
        };

        if let Some(delta) = pending_delta.as_ref() {
            *current_viewport = Some(viewport.clone());
            renderer.update(queue, delta, &viewport);
            if let Some(ref mut tr) = self.text_renderer {
                tr.update(queue, delta, &self.text_run_cache, &viewport);
            }
        }

        let raw_items = scene_graph.items();

        let has_external = !external_draws.is_empty()
            && raw_items.iter().any(|it| {
                matches!(
                    it.primitive,
                    crate::scene::primitive::Primitive::External { .. }
                )
            });

        let has_text = self.text_renderer.is_some()
            && raw_items.iter().any(|it| {
                matches!(
                    it.primitive,
                    crate::scene::primitive::Primitive::Text { .. }
                )
            });

        if !has_external && !has_text {
            renderer.encode(pass);
            return;
        }

        let mut quad_range_start: Option<u32> = None;
        let mut quad_range_end: u32 = 0;

        for item in raw_items {
            match &item.primitive {
                crate::scene::primitive::Primitive::External { draw, rect } => {
                    if let Some(start) = quad_range_start.take() {
                        let count = quad_range_end - start + 1;
                        renderer.encode_range(pass, start, count);
                        quad_range_end = 0;
                    }

                    if let Some(cb) = external_draws.get(draw) {
                        let context = ExternalDrawContext::new(*rect, viewport.clone());
                        if context.is_empty() {
                            continue;
                        }
                        let (phys_x, phys_y, phys_w, phys_h) = context.scissor_rect();
                        pass.set_scissor_rect(phys_x, phys_y, phys_w, phys_h);
                        cb(*draw, &context, pass);
                        pass.set_scissor_rect(
                            0,
                            0,
                            viewport.physical_size.0,
                            viewport.physical_size.1,
                        );
                    }
                }
                crate::scene::primitive::Primitive::Text { .. } => {
                    if let Some(start) = quad_range_start.take() {
                        let count = quad_range_end - start + 1;
                        renderer.encode_range(pass, start, count);
                        quad_range_end = 0;
                    }

                    if let Some(ref tr) = self.text_renderer {
                        tr.encode_item(pass, item.id);
                    }
                }
                _ => {
                    if let Some(slot) = renderer.slot_of(item.id) {
                        match quad_range_start {
                            None => {
                                quad_range_start = Some(slot);
                                quad_range_end = slot;
                            }
                            Some(_) => {
                                quad_range_end = quad_range_end.max(slot);
                            }
                        }
                    }
                }
            }
        }

        if let Some(start) = quad_range_start {
            let count = quad_range_end - start + 1;
            renderer.encode_range(pass, start, count);
        }
    }
}

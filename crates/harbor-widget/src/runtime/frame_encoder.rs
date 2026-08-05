//! GPU frame encoding: renderer ownership and paint-order draw batching.

use crate::renderer::Viewport;
use crate::renderer::quad::QuadRenderer;
use crate::renderer::text_renderer::TextRenderer;
use crate::scene::primitive::{ExternalDrawContext, ExternalDrawFn, ExternalDrawId};
use crate::scene::{SceneDelta, SceneGraph};
use crate::text::{GlyphFn, TextMetrics, TextRunCache};
use hashbrown::HashMap;
use std::sync::Arc;

/// Scene graph inputs required to encode one GPU frame.
pub(crate) struct EncodeScene<'a> {
    pub(crate) scene_graph: &'a SceneGraph,
    pub(crate) pending_delta: &'a mut Option<SceneDelta>,
    pub(crate) current_viewport: &'a mut Option<Viewport>,
    pub(crate) external_draws: &'a HashMap<ExternalDrawId, Arc<ExternalDrawFn<'static>>>,
}

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

    /// Prepares cached glyph layouts from the current retained scene.
    ///
    /// Scene item IDs are the cache keys, so a repeated preparation pass leaves
    /// unchanged text in place and releases entries for removed text items.
    pub(crate) fn prepare_text_runs(
        &mut self,
        scene_graph: &SceneGraph,
        metrics: &TextMetrics,
        glyph_fn: &GlyphFn<'_>,
    ) {
        let mut live_ids = Vec::new();
        for item in scene_graph.items() {
            if let crate::scene::primitive::Primitive::Text { text, .. } = &item.primitive {
                self.text_run_cache.upsert(item.id, text, metrics, glyph_fn);
                live_ids.push(item.id);
            }
        }
        self.text_run_cache.retain_live_ids(live_ids);
    }

    /// Applies a pending SceneDelta and encodes draw calls in paint order.
    pub(crate) fn encode<'a>(
        &'a mut self,
        queue: &wgpu::Queue,
        pass: &mut wgpu::RenderPass<'a>,
        viewport: Viewport,
        scene: EncodeScene<'_>,
    ) {
        let renderer = match self.renderer.as_mut() {
            Some(r) => r,
            None => return,
        };

        if let Some(delta) = scene.pending_delta.as_ref() {
            *scene.current_viewport = Some(viewport.clone());
            renderer.update(queue, delta, &viewport);
            if let Some(ref mut tr) = self.text_renderer {
                tr.update(queue, scene.scene_graph, &self.text_run_cache, &viewport);
            }
        }

        let raw_items = scene.scene_graph.items();

        let has_external = !scene.external_draws.is_empty()
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

                    if let Some(cb) = scene.external_draws.get(draw) {
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

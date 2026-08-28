//! GPU frame encoding: renderer ownership and paint-order draw batching.

use crate::renderer::Viewport;
use crate::renderer::quad::QuadRenderer;
use crate::renderer::text_renderer::TextRenderer;
use crate::scene::primitive::{
    ExternalDrawContext, ExternalDrawFn, ExternalDrawId, ExternalDrawMode, Primitive,
};
use crate::scene::{SceneDelta, SceneGraph};
use crate::text::{GlyphFn, TextMetrics, TextRunCache};
use hashbrown::HashMap;
use std::sync::Arc;

/// Scene graph inputs required to encode one GPU frame.
pub(crate) struct EncodeScene<'a> {
    pub(crate) scene_graph: &'a SceneGraph,
    pub(crate) pending_delta: &'a mut Option<SceneDelta>,
    pub(crate) external_draws: &'a HashMap<ExternalDrawId, Arc<ExternalDrawFn<'static>>>,
    pub(crate) external_eligible: &'a HashMap<ExternalDrawId, bool>,
    pub(crate) commit: bool,
}

/// Flushes a contiguous run of renderer slots without imposing paint order.
fn flush_quad_range<'a>(
    renderer: &'a QuadRenderer,
    pass: &mut wgpu::RenderPass<'a>,
    start: &mut Option<u32>,
    count: &mut u32,
    previous: &mut Option<u32>,
) {
    if let Some(start_slot) = start.take() {
        renderer.encode_range(pass, start_slot, *count);
    }
    *count = 0;
    *previous = None;
}

/// Owns GPU renderers and encodes a paint-ordered scene into a render pass.
///
/// Lifecycle: created empty with the Runtime; renderers are initialized once a
/// wgpu Device is available; encode runs each frame after the tree paint pass.
pub(crate) struct FrameEncoder {
    renderer: Option<QuadRenderer>,
    text_renderer: Option<TextRenderer>,
    text_run_cache: TextRunCache,
    encoded_viewport: Option<Viewport>,
}

impl FrameEncoder {
    pub(crate) fn new() -> Self {
        Self {
            renderer: None,
            text_renderer: None,
            text_run_cache: TextRunCache::new(),
            encoded_viewport: None,
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

        let viewport_changed = self.encoded_viewport.as_ref() != Some(&viewport);
        let delta = scene.pending_delta.take();
        if let Some(delta) = delta.as_ref() {
            renderer.update(queue, delta, &viewport);
        }
        let raw_items = scene.scene_graph.items();
        if viewport_changed {
            renderer.refresh_viewport(queue, raw_items, &viewport);
        }
        if (delta.is_some() || viewport_changed)
            && let Some(ref mut tr) = self.text_renderer
        {
            tr.update(queue, scene.scene_graph, &self.text_run_cache, &viewport);
        }
        self.encoded_viewport = Some(viewport.clone());

        // Renderer slots are retained by item ID and therefore are not a
        // paint-order representation. Walk the raw scene sequence and only
        // batch slots that are adjacent in that sequence and numerically
        // adjacent in the instance buffer.
        let mut quad_range_start: Option<u32> = None;
        let mut quad_range_count = 0u32;
        let mut previous_slot = None;

        for item in raw_items {
            match &item.primitive {
                Primitive::External { draw, rect } => {
                    flush_quad_range(
                        renderer,
                        pass,
                        &mut quad_range_start,
                        &mut quad_range_count,
                        &mut previous_slot,
                    );

                    if let Some(cb) = scene.external_draws.get(draw) {
                        let context = ExternalDrawContext::new(*rect, viewport.clone());
                        if context.is_empty() {
                            continue;
                        }
                        let (phys_x, phys_y, phys_w, phys_h) = context.scissor_rect();
                        pass.set_scissor_rect(phys_x, phys_y, phys_w, phys_h);
                        let eligible = scene.external_eligible.get(draw).copied().unwrap_or(true);
                        let mode = ExternalDrawMode::from_eligibility(eligible, scene.commit);
                        cb(*draw, &context, pass, mode);
                        pass.set_scissor_rect(
                            0,
                            0,
                            viewport.physical_size.0,
                            viewport.physical_size.1,
                        );
                    }
                }
                Primitive::Text { .. } => {
                    flush_quad_range(
                        renderer,
                        pass,
                        &mut quad_range_start,
                        &mut quad_range_count,
                        &mut previous_slot,
                    );

                    if let Some(ref tr) = self.text_renderer {
                        tr.encode_item(pass, item.id);
                    }
                }
                Primitive::Quad { .. }
                | Primitive::Border { .. }
                | Primitive::RoundedQuad { .. }
                | Primitive::RoundedBorder { .. }
                | Primitive::OuterShadow { .. } => {
                    if let Some(slot) = renderer.slot_of(item.id) {
                        if previous_slot != Some(slot.saturating_sub(1)) {
                            flush_quad_range(
                                renderer,
                                pass,
                                &mut quad_range_start,
                                &mut quad_range_count,
                                &mut previous_slot,
                            );
                            quad_range_start = Some(slot);
                        }
                        quad_range_count += 1;
                        previous_slot = Some(slot);
                    } else {
                        flush_quad_range(
                            renderer,
                            pass,
                            &mut quad_range_start,
                            &mut quad_range_count,
                            &mut previous_slot,
                        );
                    }
                }
            }
        }

        flush_quad_range(
            renderer,
            pass,
            &mut quad_range_start,
            &mut quad_range_count,
            &mut previous_slot,
        );
    }
}

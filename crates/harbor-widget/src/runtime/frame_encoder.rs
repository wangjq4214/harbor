//! GPU frame encoding: renderer ownership and paint-order draw batching.

use crate::renderer::Viewport;
use crate::renderer::quad::QuadRenderer;
use crate::renderer::text_renderer::TextRenderer;
use crate::scene::primitive::{
    ExternalDrawContext, ExternalDrawFn, ExternalDrawId, ExternalDrawMode,
};
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
    pub(crate) external_eligible: &'a HashMap<ExternalDrawId, bool>,
    pub(crate) commit: bool,
}

/// Helper that partitions a slice of slot indices into contiguous `(start, count)` ranges.
#[cfg(test)]
pub(crate) fn collect_quad_ranges(slots: &[u32]) -> Vec<(u32, u32)> {
    let mut ranges = Vec::new();
    let mut current_range: Option<(u32, u32)> = None;

    for &slot in slots {
        match current_range.as_mut() {
            None => {
                current_range = Some((slot, 1));
            }
            Some((start, count)) => {
                if slot == *start + *count {
                    *count += 1;
                } else {
                    ranges.push((*start, *count));
                    current_range = Some((slot, 1));
                }
            }
        }
    }

    if let Some(range) = current_range {
        ranges.push(range);
    }

    ranges
}

/// Represents an abstract draw command recorded during paint-order encoding.
#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum EncodedDrawCommand {
    QuadRange { start: u32, count: u32 },
    TextItem(u64),
    ExternalItem(ExternalDrawId),
}

/// Helper that simulates the exact batching and barrier-splitting logic used in `FrameEncoder::encode`.
#[cfg(test)]
pub(crate) fn plan_draw_commands(
    items: &[(u64, &crate::scene::primitive::Primitive)],
    slot_of: impl Fn(u64) -> Option<u32>,
) -> Vec<EncodedDrawCommand> {
    let mut commands = Vec::new();
    let mut quad_range_start: Option<u32> = None;
    let mut quad_range_count: u32 = 0;

    for &(id, prim) in items {
        match prim {
            crate::scene::primitive::Primitive::External { draw, .. } => {
                if let Some(start) = quad_range_start.take() {
                    commands.push(EncodedDrawCommand::QuadRange {
                        start,
                        count: quad_range_count,
                    });
                    quad_range_count = 0;
                }
                commands.push(EncodedDrawCommand::ExternalItem(*draw));
            }
            crate::scene::primitive::Primitive::Text { .. } => {
                if let Some(start) = quad_range_start.take() {
                    commands.push(EncodedDrawCommand::QuadRange {
                        start,
                        count: quad_range_count,
                    });
                    quad_range_count = 0;
                }
                commands.push(EncodedDrawCommand::TextItem(id));
            }
            _ => {
                if let Some(slot) = slot_of(id) {
                    match quad_range_start {
                        None => {
                            quad_range_start = Some(slot);
                            quad_range_count = 1;
                        }
                        Some(start) => {
                            if slot == start + quad_range_count {
                                quad_range_count += 1;
                            } else {
                                commands.push(EncodedDrawCommand::QuadRange {
                                    start,
                                    count: quad_range_count,
                                });
                                quad_range_start = Some(slot);
                                quad_range_count = 1;
                            }
                        }
                    }
                }
            }
        }
    }

    if let Some(start) = quad_range_start {
        commands.push(EncodedDrawCommand::QuadRange {
            start,
            count: quad_range_count,
        });
    }

    commands
}

/// Owns GPU renderers and encodes a paint-ordered scene into a render pass.
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

    pub(crate) fn set_text_bind_group(&mut self, bind_group: &wgpu::BindGroup) {
        if let Some(tr) = &mut self.text_renderer {
            tr.set_bind_group(bind_group);
        }
    }

    pub(crate) fn text_run_cache(&mut self) -> &mut TextRunCache {
        &mut self.text_run_cache
    }

    /// Prepares cached glyph layouts from the current retained scene.
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
            if let Some(tr) = &mut self.text_renderer {
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
        let mut quad_range_count: u32 = 0;

        for item in raw_items {
            match &item.primitive {
                crate::scene::primitive::Primitive::External { draw, rect } => {
                    if let Some(start) = quad_range_start.take() {
                        renderer.encode_range(pass, start, quad_range_count);
                        quad_range_count = 0;
                    }

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
                crate::scene::primitive::Primitive::Text { .. } => {
                    if let Some(start) = quad_range_start.take() {
                        renderer.encode_range(pass, start, quad_range_count);
                        quad_range_count = 0;
                    }

                    if let Some(tr) = &self.text_renderer {
                        tr.encode_item(pass, item.id);
                    }
                }
                _ => {
                    if let Some(slot) = renderer.slot_of(item.id) {
                        match quad_range_start {
                            None => {
                                quad_range_start = Some(slot);
                                quad_range_count = 1;
                            }
                            Some(start) => {
                                if slot == start + quad_range_count {
                                    quad_range_count += 1;
                                } else {
                                    renderer.encode_range(pass, start, quad_range_count);
                                    quad_range_start = Some(slot);
                                    quad_range_count = 1;
                                }
                            }
                        }
                    }
                }
            }
        }

        if let Some(start) = quad_range_start {
            renderer.encode_range(pass, start, quad_range_count);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{Point, Rect};
    use crate::scene::primitive::{Color, Primitive};

    #[test]
    fn collect_quad_ranges_contiguous() {
        let slots = vec![0, 1, 2, 3];
        assert_eq!(collect_quad_ranges(&slots), vec![(0, 4)]);
    }

    #[test]
    fn collect_quad_ranges_non_contiguous_and_reverse() {
        let slots = vec![5, 2, 8, 9, 1];
        assert_eq!(
            collect_quad_ranges(&slots),
            vec![(5, 1), (2, 1), (8, 2), (1, 1)]
        );
    }

    #[test]
    fn collect_quad_ranges_empty() {
        let slots: Vec<u32> = vec![];
        assert!(collect_quad_ranges(&slots).is_empty());
    }

    #[test]
    fn plan_draw_commands_preserves_quad_text_quad_interleaving() {
        // Simulates: Quad(id 1, slot 0) -> Text(id 2) -> Quad(id 3, slot 1)
        // Even though slot 0 and slot 1 are numerically contiguous, the Text item in between
        // MUST flush the first quad range so that Quad 1 is drawn BEFORE Text 2, and Quad 3 AFTER Text 2.
        let quad1 = Primitive::Quad {
            rect: Rect::from_min_size(Point::ZERO, crate::layout::Size::new(100.0, 30.0)),
            color: Color::WHITE,
            corner_radius: 0.0,
        };
        let text2 = Primitive::Text {
            text: Arc::from("Tab"),
            origin: Point::ZERO,
            color: Color::WHITE,
        };
        let quad3 = Primitive::Quad {
            rect: Rect::from_min_size(Point::new(0.0, 30.0), crate::layout::Size::new(100.0, 30.0)),
            color: Color::WHITE,
            corner_radius: 0.0,
        };

        let items = vec![(1, &quad1), (2, &text2), (3, &quad3)];
        let slot_map = |id| match id {
            1 => Some(0),
            3 => Some(1),
            _ => None,
        };

        let commands = plan_draw_commands(&items, slot_map);
        assert_eq!(
            commands,
            vec![
                EncodedDrawCommand::QuadRange { start: 0, count: 1 },
                EncodedDrawCommand::TextItem(2),
                EncodedDrawCommand::QuadRange { start: 1, count: 1 },
            ]
        );
    }

    #[test]
    fn plan_draw_commands_batches_consecutive_quads_without_barriers() {
        let quad1 = Primitive::Quad {
            rect: Rect::from_min_size(Point::ZERO, crate::layout::Size::new(10.0, 10.0)),
            color: Color::WHITE,
            corner_radius: 0.0,
        };
        let quad2 = Primitive::Quad {
            rect: Rect::from_min_size(Point::new(10.0, 0.0), crate::layout::Size::new(10.0, 10.0)),
            color: Color::WHITE,
            corner_radius: 0.0,
        };
        let quad3 = Primitive::Quad {
            rect: Rect::from_min_size(Point::new(20.0, 0.0), crate::layout::Size::new(10.0, 10.0)),
            color: Color::WHITE,
            corner_radius: 0.0,
        };

        let items = vec![(1, &quad1), (2, &quad2), (3, &quad3)];
        // Slots 5, 6, 7 are contiguous
        let slot_map = |id| match id {
            1 => Some(5),
            2 => Some(6),
            3 => Some(7),
            _ => None,
        };

        let commands = plan_draw_commands(&items, slot_map);
        assert_eq!(
            commands,
            vec![EncodedDrawCommand::QuadRange { start: 5, count: 3 }]
        );
    }

    #[test]
    fn plan_draw_commands_preserves_paint_order_for_reverse_slots() {
        let quad1 = Primitive::Quad {
            rect: Rect::from_min_size(Point::ZERO, crate::layout::Size::new(10.0, 10.0)),
            color: Color::WHITE,
            corner_radius: 0.0,
        };
        let quad2 = Primitive::Quad {
            rect: Rect::from_min_size(Point::new(10.0, 0.0), crate::layout::Size::new(10.0, 10.0)),
            color: Color::WHITE,
            corner_radius: 0.0,
        };
        let quad3 = Primitive::Quad {
            rect: Rect::from_min_size(Point::new(20.0, 0.0), crate::layout::Size::new(10.0, 10.0)),
            color: Color::WHITE,
            corner_radius: 0.0,
        };

        let items = vec![(1, &quad1), (2, &quad2), (3, &quad3)];
        let slot_map = |id| match id {
            1 => Some(5),
            2 => Some(2),
            3 => Some(3),
            _ => None,
        };

        let commands = plan_draw_commands(&items, slot_map);
        assert_eq!(
            commands,
            vec![
                EncodedDrawCommand::QuadRange { start: 5, count: 1 },
                EncodedDrawCommand::QuadRange { start: 2, count: 2 },
            ]
        );
    }
}

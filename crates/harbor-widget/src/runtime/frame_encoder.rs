//! GPU frame encoding: renderer ownership and paint-order draw batching.

use crate::decoration::ClipBehavior;
use crate::layout::Rect;
use crate::renderer::Viewport;
use crate::renderer::quad::QuadRenderer;
use crate::renderer::text_renderer::TextRenderer;
use crate::scene::clip::RoundedClip;
use crate::scene::primitive::{
    ExternalDrawContext, ExternalDrawFn, ExternalDrawId, ExternalDrawMode, Primitive,
};
use crate::scene::{SceneDelta, SceneGraph};
use crate::text::{GlyphFn, TextMetrics, TextRunCache};
use hashbrown::HashMap;
use std::sync::Arc;

/// Encoder-facing decision for one external SceneItem.
enum ExternalClipPlan {
    Skip,
    Draw {
        scissor: (u32, u32, u32, u32),
        apply_rounded_mask: bool,
    },
}

impl ExternalClipPlan {
    fn from(rect: Rect, clips: &[RoundedClip], viewport: &Viewport) -> Self {
        let mut scissor = ExternalDrawContext::compute_scissor(
            rect,
            viewport.scale_factor,
            viewport.physical_size,
        );
        if scissor.2 == 0 || scissor.3 == 0 {
            return Self::Skip;
        }
        let mut apply_rounded_mask = false;
        for clip in clips {
            if clip.behavior() == ClipBehavior::None {
                continue;
            }
            apply_rounded_mask = true;
            let clip_scissor = ExternalDrawContext::compute_scissor(
                clip.rect(),
                viewport.scale_factor,
                viewport.physical_size,
            );
            scissor = intersect_scissor(scissor, clip_scissor);
            if scissor.2 == 0 || scissor.3 == 0 {
                return Self::Skip;
            }
        }
        Self::Draw {
            scissor,
            apply_rounded_mask,
        }
    }
}

fn intersect_scissor(lhs: (u32, u32, u32, u32), rhs: (u32, u32, u32, u32)) -> (u32, u32, u32, u32) {
    let left = lhs.0.max(rhs.0);
    let top = lhs.1.max(rhs.1);
    let right = lhs.0.saturating_add(lhs.2).min(rhs.0.saturating_add(rhs.2));
    let bottom = lhs.1.saturating_add(lhs.3).min(rhs.1.saturating_add(rhs.3));
    (
        left,
        top,
        right.saturating_sub(left),
        bottom.saturating_sub(top),
    )
}

/// Callback arguments plus clip scissor for one external SceneItem.
///
/// Ancestor clips may shrink `scissor` and set `apply_rounded_mask`; they must
/// not rewrite the draw id, allocation context, or Live/Retain mode.
struct ExternalDrawInvocation {
    id: ExternalDrawId,
    context: ExternalDrawContext,
    mode: ExternalDrawMode,
    scissor: (u32, u32, u32, u32),
    apply_rounded_mask: bool,
}

fn plan_external_draw(
    id: ExternalDrawId,
    rect: Rect,
    clips: &[RoundedClip],
    viewport: &Viewport,
    eligible: bool,
    commit: bool,
) -> Option<ExternalDrawInvocation> {
    match ExternalClipPlan::from(rect, clips, viewport) {
        ExternalClipPlan::Skip => None,
        ExternalClipPlan::Draw {
            scissor,
            apply_rounded_mask,
        } => Some(ExternalDrawInvocation {
            id,
            context: ExternalDrawContext::new(rect, viewport.clone()),
            mode: ExternalDrawMode::from_eligibility(eligible, commit),
            scissor,
            apply_rounded_mask,
        }),
    }
}

/// Scene graph inputs required to encode one GPU frame.
pub(crate) struct EncodeScene<'a> {
    pub(crate) scene_graph: &'a SceneGraph,
    pub(crate) pending_delta: &'a mut Option<SceneDelta>,
    pub(crate) external_draws: &'a HashMap<ExternalDrawId, Arc<ExternalDrawFn<'static>>>,
    pub(crate) external_eligible: &'a HashMap<ExternalDrawId, bool>,
    pub(crate) commit: bool,
}

/// Tracks the contiguous run of quad-renderer slots being batched into one
/// draw call. A slot extends the run only while numerically adjacent to the
/// previous one; anything else flushes the open run first.
struct QuadRun {
    start: Option<u32>,
    count: u32,
    previous: Option<u32>,
}

impl QuadRun {
    fn new() -> Self {
        Self {
            start: None,
            count: 0,
            previous: None,
        }
    }

    fn flush<'a>(&mut self, renderer: &'a QuadRenderer, pass: &mut wgpu::RenderPass<'a>) {
        if let Some(start_slot) = self.start.take() {
            renderer.encode_range(pass, start_slot, self.count);
        }
        self.count = 0;
        self.previous = None;
    }

    fn extend<'a>(
        &mut self,
        renderer: &'a QuadRenderer,
        pass: &mut wgpu::RenderPass<'a>,
        slot: u32,
    ) {
        if self.previous != Some(slot.saturating_sub(1)) {
            self.flush(renderer, pass);
            self.start = Some(slot);
        }
        self.count += 1;
        self.previous = Some(slot);
    }
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

    pub(crate) fn set_text_bind_group(&mut self, bind_group: &wgpu::BindGroup) {
        if let Some(renderer) = &mut self.text_renderer {
            renderer.set_bind_group(bind_group);
        }
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

        // Plan every external draw once per frame. Items that survive
        // planning also seed the clip-mask upload pass, so the draw loop can
        // reuse the plan instead of recomputing it.
        let mut external_invocations: HashMap<u64, ExternalDrawInvocation> = HashMap::new();
        let mut clip_masks: Vec<(&[RoundedClip], Rect)> = Vec::new();
        for item in raw_items {
            let Primitive::External { draw, rect } = &item.primitive else {
                continue;
            };
            if !scene.external_draws.contains_key(draw) {
                continue;
            }
            let eligible = scene.external_eligible.get(draw).copied().unwrap_or(true);
            let Some(invocation) =
                plan_external_draw(*draw, *rect, &item.clips, &viewport, eligible, scene.commit)
            else {
                continue;
            };
            if invocation.apply_rounded_mask {
                clip_masks.push((item.clips.as_slice(), *rect));
            }
            external_invocations.insert(item.id, invocation);
        }
        renderer.upload_clip_masks(queue, &clip_masks, &viewport);

        // Renderer slots are retained by item ID and therefore are not a
        // paint-order representation. Walk the raw scene sequence and only
        // batch slots that are adjacent in that sequence and numerically
        // adjacent in the instance buffer.
        let mut quad_run = QuadRun::new();
        let mut next_mask_slot = 0u32;

        for item in raw_items {
            match &item.primitive {
                Primitive::External { draw, .. } => {
                    quad_run.flush(renderer, pass);
                    if let (Some(invocation), Some(cb)) = (
                        external_invocations.get(&item.id),
                        scene.external_draws.get(draw),
                    ) {
                        pass.set_scissor_rect(
                            invocation.scissor.0,
                            invocation.scissor.1,
                            invocation.scissor.2,
                            invocation.scissor.3,
                        );
                        cb(invocation.id, &invocation.context, pass, invocation.mode);
                        if invocation.apply_rounded_mask {
                            // Handlers may replace scissor; dest-in must stay on the plan.
                            pass.set_scissor_rect(
                                invocation.scissor.0,
                                invocation.scissor.1,
                                invocation.scissor.2,
                                invocation.scissor.3,
                            );
                            renderer.encode_clip_mask(pass, next_mask_slot);
                            next_mask_slot += 1;
                        }
                        pass.set_scissor_rect(
                            0,
                            0,
                            viewport.physical_size.0,
                            viewport.physical_size.1,
                        );
                    }
                }
                Primitive::Text { .. } => {
                    quad_run.flush(renderer, pass);

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
                        quad_run.extend(renderer, pass, slot);
                    } else {
                        quad_run.flush(renderer, pass);
                    }
                }
            }
        }

        quad_run.flush(renderer, pass);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decoration::BorderRadius;
    use crate::layout::{Point, Size};

    fn clip(rect: Rect, radius: f32, behavior: ClipBehavior) -> RoundedClip {
        RoundedClip::new(rect, BorderRadius::all(radius).unwrap(), behavior).unwrap()
    }

    fn overlapping_hard_edge_clip(rect: Rect) -> [RoundedClip; 1] {
        [clip(
            Rect::from_min_size(
                Point::new(rect.min.x + 10.0, rect.min.y - 10.0),
                Size::new(20.0, rect.size().height + 20.0),
            ),
            4.0,
            ClipBehavior::HardEdge,
        )]
    }

    #[test]
    fn should_draw_without_mask_when_clips_are_empty() {
        let viewport = Viewport::new(800, 600, 1.0);
        let rect = Rect::from_min_size(Point::new(10.0, 20.0), Size::new(100.0, 50.0));

        let plan = ExternalClipPlan::from(rect, &[], &viewport);

        match plan {
            ExternalClipPlan::Draw {
                scissor,
                apply_rounded_mask,
            } => {
                assert_eq!(scissor, (10, 20, 100, 50));
                assert!(!apply_rounded_mask);
            }
            ExternalClipPlan::Skip => panic!("drawable allocation must not skip"),
        }
    }

    #[test]
    fn should_ignore_none_clips_when_planning_external_mask() {
        let viewport = Viewport::new(800, 600, 1.0);
        let rect = Rect::from_min_size(Point::ZERO, Size::new(40.0, 40.0));
        let clips = [clip(rect, 8.0, ClipBehavior::None)];

        let plan = ExternalClipPlan::from(rect, &clips, &viewport);

        match plan {
            ExternalClipPlan::Draw {
                apply_rounded_mask, ..
            } => assert!(!apply_rounded_mask),
            ExternalClipPlan::Skip => panic!("None clip must not skip a drawable allocation"),
        }
    }

    #[test]
    fn should_set_mask_when_hard_edge_or_anti_alias_clip_is_active() {
        let viewport = Viewport::new(800, 600, 1.0);
        let rect = Rect::from_min_size(Point::ZERO, Size::new(40.0, 40.0));

        for behavior in [ClipBehavior::HardEdge, ClipBehavior::AntiAlias] {
            let clips = [clip(rect, 8.0, behavior)];
            match ExternalClipPlan::from(rect, &clips, &viewport) {
                ExternalClipPlan::Draw {
                    apply_rounded_mask, ..
                } => assert!(apply_rounded_mask),
                ExternalClipPlan::Skip => panic!("{behavior:?} clip must draw with a mask"),
            }
        }
    }

    #[test]
    fn should_skip_when_allocation_or_surface_is_empty() {
        let rect = Rect::from_min_size(Point::ZERO, Size::new(40.0, 40.0));
        assert!(matches!(
            ExternalClipPlan::from(rect, &[], &Viewport::new(0, 256, 1.0)),
            ExternalClipPlan::Skip
        ));
        let empty = Rect::from_min_size(Point::ZERO, Size::ZERO);
        assert!(matches!(
            ExternalClipPlan::from(empty, &[], &Viewport::new(256, 256, 1.0)),
            ExternalClipPlan::Skip
        ));
    }

    #[test]
    fn should_skip_when_ancestor_clip_aabb_does_not_overlap_allocation() {
        let viewport = Viewport::new(800, 600, 1.0);
        let rect = Rect::from_min_size(Point::new(100.0, 100.0), Size::new(40.0, 40.0));
        let clips = [clip(
            Rect::from_min_size(Point::ZERO, Size::new(20.0, 20.0)),
            4.0,
            ClipBehavior::HardEdge,
        )];

        assert!(matches!(
            ExternalClipPlan::from(rect, &clips, &viewport),
            ExternalClipPlan::Skip
        ));
    }

    #[test]
    fn should_intersect_scissor_with_overlapping_ancestor_clip_aabb() {
        let viewport = Viewport::new(800, 600, 1.0);
        let rect = Rect::from_min_size(Point::new(10.0, 10.0), Size::new(40.0, 40.0));
        let clips = [clip(
            Rect::from_min_size(Point::new(20.0, 0.0), Size::new(20.0, 80.0)),
            4.0,
            ClipBehavior::HardEdge,
        )];

        match ExternalClipPlan::from(rect, &clips, &viewport) {
            ExternalClipPlan::Draw {
                scissor,
                apply_rounded_mask,
            } => {
                assert_eq!(scissor, (20, 10, 20, 40));
                assert!(apply_rounded_mask);
            }
            ExternalClipPlan::Skip => panic!("overlapping clip AABB must remain drawable"),
        }
    }

    #[test]
    fn should_intersect_scissor_at_one_and_a_half_and_two_times_scale() {
        let rect = Rect::from_min_size(Point::new(10.0, 10.0), Size::new(20.0, 20.0));
        let clip_rect = Rect::from_min_size(Point::new(15.0, 0.0), Size::new(20.0, 40.0));
        let clips = [clip(clip_rect, 4.0, ClipBehavior::AntiAlias)];

        match ExternalClipPlan::from(rect, &clips, &Viewport::new(800, 600, 2.0)) {
            ExternalClipPlan::Draw { scissor, .. } => {
                assert_eq!(scissor, (30, 20, 30, 40));
            }
            ExternalClipPlan::Skip => panic!("2x overlapping clip must remain drawable"),
        }

        match ExternalClipPlan::from(rect, &clips, &Viewport::new(800, 600, 1.5)) {
            ExternalClipPlan::Draw { scissor, .. } => {
                // 10*1.5=15, 30*1.5=45; clip left 15*1.5=22.5鈫?2, right 35*1.5=52.5鈫?3
                assert_eq!(scissor, (22, 15, 23, 30));
            }
            ExternalClipPlan::Skip => panic!("1.5x overlapping clip must remain drawable"),
        }
    }

    #[test]
    fn should_draw_without_mask_when_none_clip_aabb_is_disjoint() {
        // Arrange
        let viewport = Viewport::new(800, 600, 1.0);
        let rect = Rect::from_min_size(Point::new(100.0, 100.0), Size::new(40.0, 40.0));
        let clips = [clip(
            Rect::from_min_size(Point::ZERO, Size::new(20.0, 20.0)),
            4.0,
            ClipBehavior::None,
        )];

        // Act
        let plan = ExternalClipPlan::from(rect, &clips, &viewport);

        // Assert
        match plan {
            ExternalClipPlan::Draw {
                scissor,
                apply_rounded_mask,
            } => {
                assert_eq!(scissor, (100, 100, 40, 40));
                assert!(!apply_rounded_mask);
            }
            ExternalClipPlan::Skip => panic!("None clip must not skip a drawable allocation"),
        }
    }

    #[test]
    fn should_keep_callback_id_and_geometry_when_rounded_clip_is_active() {
        // Arrange
        let viewport = Viewport::new(800, 600, 2.0);
        let rect = Rect::from_min_size(Point::new(10.0, 20.0), Size::new(40.0, 30.0));
        let clips = overlapping_hard_edge_clip(rect);
        let draw_id = 7;

        // Act
        let invocation = plan_external_draw(draw_id, rect, &clips, &viewport, true, false)
            .expect("overlapping rounded clip must remain drawable");

        // Assert
        assert_eq!(invocation.id, draw_id);
        assert_eq!(invocation.context.logical_rect, rect);
        assert_eq!(invocation.context.surface_size(), (800, 600));
        assert_eq!(invocation.context.scale_factor(), 2.0);
        assert!(invocation.apply_rounded_mask);
        assert_ne!(invocation.scissor, invocation.context.scissor_rect());
    }

    #[test]
    fn should_use_live_mode_when_eligible_and_rounded_clip_is_active() {
        // Arrange
        let viewport = Viewport::new(800, 600, 1.0);
        let rect = Rect::from_min_size(Point::new(10.0, 10.0), Size::new(40.0, 40.0));
        let clips = overlapping_hard_edge_clip(rect);

        // Act
        let invocation = plan_external_draw(1, rect, &clips, &viewport, true, false)
            .expect("overlapping rounded clip must remain drawable");

        // Assert
        assert_eq!(invocation.mode, ExternalDrawMode::Live);
    }

    #[test]
    fn should_use_retain_mode_when_ineligible_without_commit_and_rounded_clip_is_active() {
        // Arrange
        let viewport = Viewport::new(800, 600, 1.0);
        let rect = Rect::from_min_size(Point::new(10.0, 10.0), Size::new(40.0, 40.0));
        let clips = overlapping_hard_edge_clip(rect);

        // Act
        let invocation = plan_external_draw(1, rect, &clips, &viewport, false, false)
            .expect("overlapping rounded clip must remain drawable");

        // Assert
        assert_eq!(invocation.mode, ExternalDrawMode::Retain);
    }

    #[test]
    fn should_use_live_mode_when_commit_overrides_ineligible_and_rounded_clip_is_active() {
        // Arrange
        let viewport = Viewport::new(800, 600, 1.0);
        let rect = Rect::from_min_size(Point::new(10.0, 10.0), Size::new(40.0, 40.0));
        let clips = overlapping_hard_edge_clip(rect);

        // Act
        let invocation = plan_external_draw(1, rect, &clips, &viewport, false, true)
            .expect("overlapping rounded clip must remain drawable");

        // Assert
        assert_eq!(invocation.mode, ExternalDrawMode::Live);
    }

    #[test]
    fn should_omit_callback_when_clip_plan_skips() {
        // Arrange
        let viewport = Viewport::new(800, 600, 1.0);
        let rect = Rect::from_min_size(Point::new(100.0, 100.0), Size::new(40.0, 40.0));
        let clips = [clip(
            Rect::from_min_size(Point::ZERO, Size::new(20.0, 20.0)),
            4.0,
            ClipBehavior::HardEdge,
        )];

        // Act
        let invocation = plan_external_draw(1, rect, &clips, &viewport, true, false);

        // Assert
        assert!(invocation.is_none());
    }

    #[test]
    fn should_intersect_scissor_when_two_ancestor_clips_overlap() {
        // Arrange
        let viewport = Viewport::new(800, 600, 1.0);
        let rect = Rect::from_min_size(Point::new(10.0, 10.0), Size::new(40.0, 40.0));
        let clips = [
            clip(
                Rect::from_min_size(Point::new(20.0, 0.0), Size::new(20.0, 80.0)),
                4.0,
                ClipBehavior::HardEdge,
            ),
            clip(
                Rect::from_min_size(Point::new(0.0, 20.0), Size::new(80.0, 20.0)),
                4.0,
                ClipBehavior::AntiAlias,
            ),
        ];

        // Act
        let plan = ExternalClipPlan::from(rect, &clips, &viewport);

        // Assert
        match plan {
            ExternalClipPlan::Draw {
                scissor,
                apply_rounded_mask,
            } => {
                assert_eq!(scissor, (20, 20, 20, 20));
                assert!(apply_rounded_mask);
            }
            ExternalClipPlan::Skip => panic!("nested overlapping clips must remain drawable"),
        }
    }

    #[test]
    fn should_intersect_only_active_clips_when_none_clip_is_mixed_in() {
        // Arrange
        let viewport = Viewport::new(800, 600, 1.0);
        let rect = Rect::from_min_size(Point::new(10.0, 10.0), Size::new(40.0, 40.0));
        let clips = [
            clip(
                Rect::from_min_size(Point::ZERO, Size::new(5.0, 5.0)),
                1.0,
                ClipBehavior::None,
            ),
            clip(
                Rect::from_min_size(Point::new(20.0, 0.0), Size::new(20.0, 80.0)),
                4.0,
                ClipBehavior::HardEdge,
            ),
        ];

        // Act
        let plan = ExternalClipPlan::from(rect, &clips, &viewport);

        // Assert
        match plan {
            ExternalClipPlan::Draw {
                scissor,
                apply_rounded_mask,
            } => {
                assert_eq!(scissor, (20, 10, 20, 40));
                assert!(apply_rounded_mask);
            }
            ExternalClipPlan::Skip => {
                panic!("disjoint None clip must not skip an overlapping HardEdge clip")
            }
        }
    }
}

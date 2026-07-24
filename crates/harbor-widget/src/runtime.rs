use crate::fiber::{
    DirtyFlags, Fiber, FiberArena, FiberId, layout_fiber, paint_fiber, reconcile_children,
    unmount_fiber,
};
use crate::layout::{BoxConstraints, Point, Size};
use crate::renderer::Viewport;
use crate::renderer::quad::QuadRenderer;
use crate::scene::{SceneDelta, SceneGraph};
use crate::signal::PENDING_DIRTY;
use crate::view::{BuildCx, Component};
use std::time::Instant;

// ── FrameRequest ────────────────────────────────────────────────────────────

/// Post-update signal indicating whether a redraw is needed.
pub struct FrameRequest {
    pub needs_redraw: bool,
}

// ── Runtime ─────────────────────────────────────────────────────────────────

/// Top-level widget tree scheduler.
///
/// Owns the fiber tree and orchestrates reconcile -> layout -> paint cycles.
pub struct Runtime {
    arena: FiberArena,
    root_id: Option<FiberId>,
    root_component: Option<Box<dyn Component>>,
    scene_graph: SceneGraph,
    renderer: Option<QuadRenderer>,
    pending_delta: Option<SceneDelta>,
    current_viewport: Option<Viewport>,
}

impl Default for Runtime {
    fn default() -> Self {
        Self::new()
    }
}

impl Runtime {
    pub fn new() -> Self {
        Runtime {
            arena: FiberArena::new(),
            root_id: None,
            root_component: None,
            scene_graph: SceneGraph::new(),
            renderer: None,
            pending_delta: None,
            current_viewport: None,
        }
    }

    /// Sets the root component and performs the initial build + layout.
    ///
    /// If a previous root existed, it is unmounted recursively.
    pub fn set_root(&mut self, root: impl Component + 'static) {
        // Unmount old root if present
        if let Some(old_root) = self.root_id.take() {
            unmount_fiber(&mut self.arena, old_root);
        }

        // Create a temporary root fiber
        let root_fiber = Fiber::new(
            None,
            std::any::TypeId::of::<()>(), // placeholder, updated below
            None,
        );
        let root_id = self.arena.insert(root_fiber);
        self.root_id = Some(root_id);

        // Take hooks (empty for new fiber) and build
        let hooks = std::mem::take(&mut self.arena.get_mut(root_id).unwrap().hooks);
        let mut cx = BuildCx {
            current_fiber: Some(root_id),
            hooks,
            hook_index: 0,
        };

        let root_boxed: Box<dyn Component> = Box::new(root);
        let view = root_boxed.build(&mut cx);
        self.root_component = Some(root_boxed);

        // Extract View data
        let widget_type = view.widget_type();
        let key = view.key().cloned();
        let (inner, children, _explicit_key) = view.decompose();

        // Update root fiber
        if let Some(fiber) = self.arena.get_mut(root_id) {
            fiber.hooks = cx.hooks;
            fiber.key = key;
            fiber.widget_type = widget_type;
            fiber.view = Some(inner);
            fiber.flags.insert(DirtyFlags::BUILD_DIRTY);
            fiber.flags.insert(DirtyFlags::LAYOUT_DIRTY);
        }

        // Reconcile children
        let new_children = reconcile_children(&mut self.arena, root_id, &[], children);
        if let Some(fiber) = self.arena.get_mut(root_id) {
            fiber.children = new_children;
        }

        // Initial layout using current viewport or default
        let viewport_size = self
            .current_viewport
            .as_ref()
            .map(|v| v.logical_size)
            .unwrap_or(Size::new(800.0, 600.0));
        let constraints = BoxConstraints::loose(viewport_size);
        layout_fiber(&mut self.arena, root_id, constraints, Point::ZERO);
        if let Some(fiber) = self.arena.get_mut(root_id) {
            fiber.flags.remove(DirtyFlags::LAYOUT_DIRTY);
            fiber.flags.remove(DirtyFlags::BUILD_DIRTY);
        }

        // Run initial paint pass
        self.run_paint_pass();

        // Mark root dirty (first update will trigger a redraw)
        crate::signal::mark_dirty(root_id);
    }

    /// Processes dirty fibers and runs layout.
    ///
    /// In Phase 0, any dirty notification triggers a full rebuild from root.
    /// Returns a `FrameRequest` indicating whether a redraw is needed.
    pub fn update(&mut self, _now: Instant) -> FrameRequest {
        // Drain the dirty queue
        let dirty: Vec<FiberId> = PENDING_DIRTY.with(|q| std::mem::take(&mut *q.borrow_mut()));

        if dirty.is_empty() {
            return FrameRequest {
                needs_redraw: false,
            };
        }

        let Some(root_id) = self.root_id else {
            return FrameRequest {
                needs_redraw: false,
            };
        };

        // Clone old children before any mutable borrows
        let old_children = self
            .arena
            .get(root_id)
            .map(|f| f.children.clone())
            .unwrap_or_default();

        // Take hooks and rebuild
        let hooks = std::mem::take(&mut self.arena.get_mut(root_id).unwrap().hooks);
        let mut cx = BuildCx {
            current_fiber: Some(root_id),
            hooks,
            hook_index: 0,
        };

        let view = self.root_component.as_ref().unwrap().build(&mut cx);
        let widget_type = view.widget_type();
        let key = view.key().cloned();
        let (inner, children, _explicit_key) = view.decompose();

        // Update root fiber
        if let Some(fiber) = self.arena.get_mut(root_id) {
            fiber.hooks = cx.hooks;
            fiber.key = key;
            fiber.widget_type = widget_type;
            fiber.view = Some(inner);
            fiber.flags.remove(DirtyFlags::BUILD_DIRTY);
        }

        // Reconcile children
        let new_children = reconcile_children(&mut self.arena, root_id, &old_children, children);
        if let Some(fiber) = self.arena.get_mut(root_id) {
            fiber.children = new_children;
        }

        // Layout pass using current viewport or default
        let viewport_size = self
            .current_viewport
            .as_ref()
            .map(|v| v.logical_size)
            .unwrap_or(Size::new(800.0, 600.0));
        let constraints = BoxConstraints::loose(viewport_size);
        layout_fiber(&mut self.arena, root_id, constraints, Point::ZERO);
        if let Some(fiber) = self.arena.get_mut(root_id) {
            fiber.flags.remove(DirtyFlags::LAYOUT_DIRTY);
        }

        // Run paint pass
        self.run_paint_pass();

        FrameRequest { needs_redraw: true }
    }

    /// Initializes the GPU renderer. Must be called after a wgpu Device is
    /// available and before the first call to `encode()`.
    pub fn init_renderer(&mut self, device: &wgpu::Device, format: wgpu::TextureFormat) {
        self.renderer = Some(QuadRenderer::new(device, format));
    }

    /// Applies the pending SceneDelta to the GPU renderer and encodes draw
    /// calls into the RenderPass. No-op if the renderer hasn't been
    /// initialized or there is no pending delta.
    pub fn encode<'a>(
        &'a mut self,
        queue: &wgpu::Queue,
        pass: &mut wgpu::RenderPass<'a>,
        viewport: Viewport,
    ) {
        if let Some(ref mut renderer) = self.renderer {
            if let Some(ref delta) = self.pending_delta {
                self.current_viewport = Some(viewport.clone());
                renderer.update(queue, delta, &viewport);
            }
            renderer.encode(pass);
        }
    }

    /// Signals that the viewport has changed (e.g., due to window resize).
    /// Marks the root fiber as LAYOUT_DIRTY so the next update re-lays out
    /// at the new size.
    pub fn set_viewport(&mut self, viewport: Viewport) {
        self.current_viewport = Some(viewport);
        if let Some(root_id) = self.root_id {
            if let Some(fiber) = self.arena.get_mut(root_id) {
                fiber.flags.insert(DirtyFlags::LAYOUT_DIRTY);
            }
            crate::signal::mark_dirty(root_id);
        }
    }

    // ── Internal ─────────────────────────────────────────────────────────

    fn run_paint_pass(&mut self) {
        let Some(root_id) = self.root_id else {
            return;
        };

        let items = paint_fiber(&self.arena, root_id, 0);
        let delta = self.scene_graph.diff(items);
        self.pending_delta = Some(delta);
    }

    // ── Accessors ──────────────────────────────────────────────────────

    /// Returns the root FiberId, if a root component has been set.
    pub fn root_id(&self) -> Option<FiberId> {
        self.root_id
    }

    /// Returns a reference to the FiberArena.
    pub fn arena(&self) -> &FiberArena {
        &self.arena
    }

    /// Returns the pending SceneDelta, if any.
    pub fn pending_delta(&self) -> Option<&SceneDelta> {
        self.pending_delta.as_ref()
    }
}

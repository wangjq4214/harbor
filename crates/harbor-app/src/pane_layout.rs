//! Application split-tree projection and identity-qualified allocation exchange.
use crate::{
    tab_manager::{PaneBinding, PaneId},
    terminal_view::{TerminalDecorationPreset, TerminalWidgetBridge, terminal_widget},
};
use harbor_widget::{
    Component as _, ComponentExt as _, LayoutObserver, SplitAxis, SplitController, SplitPane,
    layout::Rect,
    scene::primitive::ExternalDrawId,
    view::{BuildCx, View},
};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub(crate) enum PaneTree {
    Leaf(PaneId),
    Split {
        axis: SplitAxis,
        ratio: SplitController,
        first: Box<Self>,
        second: Box<Self>,
    },
}
impl PaneTree {
    pub(crate) fn split(&mut self, source: PaneId, target: PaneId, axis: SplitAxis) -> bool {
        match self {
            Self::Leaf(id) if *id == source => {
                *self = Self::Split {
                    axis,
                    ratio: SplitController::new(0.5),
                    first: Box::new(Self::Leaf(source)),
                    second: Box::new(Self::Leaf(target)),
                };
                true
            }
            Self::Leaf(_) => false,
            Self::Split { first, second, .. } => {
                first.split(source, target, axis) || second.split(source, target, axis)
            }
        }
    }
    pub(crate) fn remove(self, pane: PaneId) -> Option<Self> {
        match self {
            Self::Leaf(id) => (id != pane).then_some(Self::Leaf(id)),
            Self::Split {
                axis,
                ratio,
                first,
                second,
            } => match (first.remove(pane), second.remove(pane)) {
                (Some(first), Some(second)) => Some(Self::Split {
                    axis,
                    ratio,
                    first: Box::new(first),
                    second: Box::new(second),
                }),
                (first, second) => first.or(second),
            },
        }
    }
}

/// Read-only application projection. Engines are owned by TabManager, never this tree.
#[derive(Clone)]
pub enum PaneLayout {
    Leaf {
        binding: PaneBinding,
        bridge: TerminalWidgetBridge,
    },
    Split {
        axis: SplitAxis,
        ratio: SplitController,
        first: Box<Self>,
        second: Box<Self>,
    },
}
impl PaneLayout {
    pub fn leaves(&self) -> Vec<(PaneBinding, ExternalDrawId)> {
        match self {
            Self::Leaf { binding, bridge } => vec![(*binding, bridge.draw_id())],
            Self::Split { first, second, .. } => {
                let mut leaves = first.leaves();
                leaves.extend(second.leaves());
                leaves
            }
        }
    }
    pub(crate) fn build(
        &self,
        cx: &mut BuildCx,
        allocations: &PaneAllocations,
        focused: PaneId,
        focus: harbor_widget::FocusHandle,
    ) -> View {
        match self {
            Self::Leaf { binding, bridge } => {
                let observer = allocations.observer(*binding, bridge.draw_id());
                let leaf = terminal_widget(bridge.clone()).build(cx);
                let leaf = if binding.pane == focused {
                    harbor_widget::Focus::new()
                        .focus_handle(focus)
                        .child(leaf)
                        .build(cx)
                } else {
                    leaf
                };
                observer
                    .child(TerminalDecorationPreset::container().child(leaf))
                    .keyed(format!(
                        "pane-{}-{}",
                        binding.pane.0,
                        allocations.generation()
                    ))
                    .build(cx)
            }
            Self::Split {
                axis,
                ratio,
                first,
                second,
            } => {
                let first = first.build(cx, allocations, focused, focus);
                let second = second.build(cx, allocations, focused, focus);
                SplitPane::new(*axis, ratio.clone(), first, second).build(cx)
            }
        }
    }
}

/// Allocation reports carry both the original binding and external registration.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PaneAllocation {
    pub binding: PaneBinding,
    pub draw_id: ExternalDrawId,
    pub rect: Rect,
}
#[derive(Default)]
struct AllocationState {
    generation: u64,
    visible: Vec<(PaneBinding, ExternalDrawId)>,
    observed: Vec<PaneAllocation>,
}
#[derive(Clone, Default)]
pub(crate) struct PaneAllocations(Arc<Mutex<AllocationState>>);
impl PaneAllocations {
    pub(crate) fn publish(&self, layout: Option<&PaneLayout>) {
        let visible = layout.map_or_else(Vec::new, PaneLayout::leaves);
        let mut state = self.0.lock().expect("pane allocations");
        if state.visible != visible {
            state.generation = state
                .generation
                .checked_add(1)
                .expect("allocation generation exhausted");
            state.visible = visible;
            state.observed.clear();
        }
    }
    fn generation(&self) -> u64 {
        self.0.lock().expect("pane allocations").generation
    }
    pub(crate) fn latest(&self) -> Vec<PaneAllocation> {
        self.0.lock().expect("pane allocations").observed.clone()
    }
    fn observer(&self, binding: PaneBinding, draw_id: ExternalDrawId) -> LayoutObserver {
        let generation = self.generation();
        let shared = self.clone();
        LayoutObserver::new(move |rect| {
            let mut state = shared.0.lock().expect("pane allocations");
            if generation != state.generation || !state.visible.contains(&(binding, draw_id)) {
                return;
            }
            state
                .observed
                .retain(|allocation| allocation.binding != binding);
            state.observed.push(PaneAllocation {
                binding,
                draw_id,
                rect,
            });
        })
    }
}

#[cfg(test)]
mod tests;

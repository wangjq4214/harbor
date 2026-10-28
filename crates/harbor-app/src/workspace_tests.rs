//! Integrated application workspace regressions.
#[path = "workspace_tests/input.rs"]
mod input;
#[path = "workspace_tests/notifications.rs"]
mod notifications;
#[path = "workspace_tests/search.rs"]
mod search;
#[path = "workspace_tests/tabs.rs"]
mod tabs;

#[cfg(test)]
pub(super) fn ctrl() -> Modifiers {
    Modifiers {
        ctrl: true,
        ..Modifiers::default()
    }
}
use super::*;
use crate::command::{AppCommand, AppCommandRequest};
use crate::tab_manager::{TabId, TabIndex};
use crate::tab_view::{
    COMPACT_RAIL_WIDTH, RailPresentation, TabCommand, TabFocusPolicy, ui::abbreviation,
};
use harbor_terminal::Terminal;
use harbor_widget::{
    ConstrainedBox, DecoratedBox, Row,
    fiber::FiberId,
    input::event::{
        Key, KeyboardEvent, Modifiers, PointerButton, PointerEvent, PointerPhase, UiEvent,
    },
    layout::Point,
    renderer::Viewport,
    runtime::Runtime,
    scene::primitive::{ExternalDrawId, Primitive},
};
use std::{any::TypeId, sync::atomic::AtomicBool, time::Instant};

fn snapshot(id: u64, active: bool, unread: bool) -> TabSnapshot {
    TabSnapshot {
        id: TabId(id),
        title: format!("Terminal {id}"),
        unread,
        draw_id: ExternalDrawId::new(id),
        working_directory: None,
        shell_integration: None,
        active,
    }
}

#[allow(clippy::arc_with_non_send_sync)]
fn bridge(draw_id: u64) -> TerminalWidgetBridge {
    TerminalWidgetBridge::new(
        ExternalDrawId::new(draw_id),
        Arc::new(Mutex::new(Terminal::new_headless(4, 20))),
        Arc::new(AtomicBool::new(false)),
    )
}

fn controller(width: f64, snapshots: Vec<TabSnapshot>, active_draw: u64) -> WorkspaceUiController {
    WorkspaceUiController::new(snapshots, bridge(active_draw), width)
}

fn mount(controller: WorkspaceUiController, width: u32, height: u32) -> Runtime {
    let mut runtime = Runtime::new();
    runtime.set_viewport(Viewport::new(width, height, 1.0));
    runtime.set_root(tab_workspace(controller, false));
    runtime.update(Instant::now());
    runtime
}
#[derive(Clone)]
struct Writer(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);
impl std::io::Write for Writer {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn fiber_ids(runtime: &Runtime) -> Vec<FiberId> {
    let mut pending = vec![runtime.root_id().expect("workspace root")];
    let mut ids = Vec::new();
    while let Some(id) = pending.pop() {
        let fiber = runtime.arena().get(id).expect("live fiber");
        pending.extend_from_slice(fiber.children());
        ids.push(id);
    }
    ids
}

fn tab_rows(runtime: &Runtime) -> Vec<FiberId> {
    let mut rows: Vec<_> = fiber_ids(runtime)
        .into_iter()
        .filter(|id| {
            let fiber = runtime.arena().get(*id).expect("live fiber");
            let children = fiber.children();
            let slot_wrapped = fiber
                .key()
                .is_some_and(|key| key.text() == harbor_widget::CONTENT_SLOT_KEY);
            (fiber.widget_type() == TypeId::of::<Row>() || slot_wrapped)
                && children.last().is_some_and(|child| {
                    let kind = runtime.arena().get(*child).unwrap().widget_type();
                    (kind == TypeId::of::<ConstrainedBox>() && children.len() == 3)
                        || (kind == TypeId::of::<harbor_widget::Expanded>()
                            && runtime
                                .arena()
                                .get(*child)
                                .unwrap()
                                .children()
                                .first()
                                .is_some_and(|label| {
                                    runtime.arena().get(*label).unwrap().widget_type()
                                        == TypeId::of::<harbor_widget::DecoratedBox>()
                                }))
                })
        })
        .collect();
    rows.sort_by(|left, right| {
        let left_y = runtime
            .arena()
            .get(*left)
            .unwrap()
            .layout_rect()
            .unwrap()
            .min
            .y;
        let right_y = runtime
            .arena()
            .get(*right)
            .unwrap()
            .layout_rect()
            .unwrap()
            .min
            .y;
        left_y.total_cmp(&right_y)
    });
    rows
}

fn move_pointer(runtime: &mut Runtime, position: Point) {
    runtime.dispatch(UiEvent::Pointer(PointerEvent::new(
        position,
        PointerPhase::Move,
        PointerButton::Left,
        7,
    )));
    runtime.update(Instant::now());
}

fn close_is_visible(runtime: &Runtime, row: FiberId) -> bool {
    let rect = runtime.arena().get(row).unwrap().layout_rect().unwrap();
    let delta = runtime.pending_delta().unwrap();
    delta.added.iter().chain(&delta.modified).any(|item| {
            matches!(&item.primitive, Primitive::Icon { icon: harbor_widget::LucideIcon::X, rect: icon_rect, color }
                if rect.contains(icon_rect.min) && color.a > 0.0)
        })
}

fn search_panel_rect(runtime: &Runtime) -> Rect {
    fiber_ids(runtime)
        .into_iter()
        .find_map(|id| {
            let fiber = runtime.arena().get(id).unwrap();
            let rect = fiber.layout_rect()?;
            (fiber.widget_type() == TypeId::of::<DecoratedBox>()
                && rect.size().height == SEARCH_PANEL_HEIGHT)
                .then_some(rect)
        })
        .expect("floating search card")
}

fn search_button_surfaces(runtime: &Runtime) -> Vec<(Rect, f32)> {
    let delta = runtime.pending_delta().unwrap();
    let items: Vec<_> = delta.added.iter().chain(&delta.modified).collect();
    items
        .iter()
        .filter_map(|item| match item.primitive {
            Primitive::BackdropMaterial {
                rect,
                sigma: 2.0,
                refraction,
                corner_radii,
                ..
            } if rect.size() == harbor_widget::layout::Size::new(32.0, 32.0) => {
                assert_eq!(refraction.width(), 6.0);
                assert_eq!(refraction.displacement(), 2.0);
                assert_eq!(corner_radii, [12.0; 4]);
                let alpha = items
                    .iter()
                    .find_map(|item| match item.primitive {
                        Primitive::RoundedQuad {
                            rect: surface,
                            color,
                            ..
                        } if surface == rect => Some(color.a),
                        _ => None,
                    })
                    .expect("glass has a translucent surface");
                Some((rect, alpha))
            }
            _ => None,
        })
        .collect()
}

#[path = "palette_tests.rs"]
mod palette_support;

#[path = "workspace_tests/palette_commands.rs"]
mod palette_commands;
#[path = "workspace_tests/palette_input.rs"]
mod palette_input;
#[path = "workspace_tests/palette_layout.rs"]
mod palette_layout;
#[path = "palette_render_tests.rs"]
mod palette_render;
#[path = "workspace_tests/palette_scroll.rs"]
mod palette_scroll;
#[path = "popup_tests.rs"]
mod popups;
#[path = "palette_terminal_tests.rs"]
mod terminal_actions;

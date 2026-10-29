//! Shared palette fixtures for sibling workspace suites.
use super::*;
pub(super) use crate::{
    command::{CommandSource, command_availability, validate_keybindings},
    tab_manager::{TabManager, TerminalTabResources},
};
pub(super) use harbor_widget::{
    TextEditingController,
    effects::ClipboardEffect,
    input::event::{FocusEvent, ImePreedit},
};

pub(super) type Capture = Arc<Mutex<Vec<u8>>>;

#[allow(clippy::arc_with_non_send_sync)]
pub(super) fn resources(draw: ExternalDrawId, bytes: &Capture) -> TerminalTabResources {
    let engine = Arc::new(Mutex::new(Terminal::new_headless_with_io(
        4,
        20,
        std::io::empty(),
        Writer(bytes.clone()),
        || true,
    )));
    let bridge = TerminalWidgetBridge::new(draw, engine.clone(), Arc::new(AtomicBool::new(false)));
    TerminalTabResources::new(engine, bridge, "test")
}

pub(super) fn fixture() -> (TabManager, WorkspaceUiController, Runtime, Capture) {
    let bytes = Arc::new(Mutex::new(Vec::new()));
    let mut tabs = TabManager::new();
    tabs.create_tab(|_, draw| Ok(resources(draw, &bytes)))
        .unwrap();
    let controller =
        WorkspaceUiController::new(tabs.snapshots(), tabs.active_bridge().unwrap(), 1000.0);
    let mut runtime = mount(controller.clone(), 1000, 500);
    runtime.request_focus(&controller.terminal_focus());
    (tabs, controller, runtime, bytes)
}

pub(super) fn down(runtime: &mut Runtime, key: Key, modifiers: Modifiers) {
    runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown { key, modifiers }));
}
pub(super) fn up(runtime: &mut Runtime, key: Key, modifiers: Modifiers) {
    runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyUp { key, modifiers }));
}
pub(super) fn tap(runtime: &mut Runtime, key: Key) {
    down(runtime, key, Modifiers::default());
    up(runtime, key, Modifiers::default());
}
pub(super) fn open(controller: &WorkspaceUiController, runtime: &mut Runtime) {
    let mods = Modifiers {
        ctrl: true,
        shift: true,
        ..Modifiers::default()
    };
    down(runtime, Key::Character('P'), mods);
    let actions = controller.drain_actions();
    assert_eq!(
        actions,
        [AppCommandRequest::shortcut(AppCommand::OpenCommandPalette)]
    );
    let focus = controller.open_palette();
    runtime.request_focus(&focus);
    let effects = runtime.update(Instant::now());
    assert_eq!(
        effects.ime.expect("query IME destination").allowed,
        Some(true)
    );
    up(runtime, Key::Character('P'), mods);
    assert!(runtime.input().focused().is_some());
}
pub(super) fn input(controller: &WorkspaceUiController) -> TextEditingController {
    controller
        .store
        .state()
        .read()
        .popups
        .get::<palette::PaletteState>()
        .unwrap()
        .input
        .clone()
}
pub(super) fn replace(runtime: &mut Runtime, text: &str) {
    down(runtime, Key::Character('a'), ctrl());
    up(runtime, Key::Character('a'), ctrl());
    runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::Ime(text.into())));
    runtime.update(Instant::now());
}
pub(super) fn painted(runtime: &Runtime) -> String {
    let delta = runtime.pending_delta().unwrap();
    // Added and modified items are separate lists, not a single paint-order stream.
    let mut items: Vec<_> = delta.added.iter().chain(&delta.modified).collect();
    items.sort_by_key(|item| item.paint_order);
    items
        .iter()
        .filter_map(|item| match &item.primitive {
            Primitive::Text { text, .. } => Some(text.as_ref()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) fn take_one(controller: &WorkspaceUiController) -> AppCommandRequest {
    let actions = controller.drain_actions();
    assert_eq!(actions.len(), 1, "exactly one typed request");
    actions[0]
}

pub(super) fn palette_geometry(runtime: &Runtime) -> (Rect, Rect) {
    let delta = runtime.pending_delta().unwrap();
    let items: Vec<_> = delta.added.iter().chain(&delta.modified).collect();
    let panel = items
        .iter()
        .find_map(|item| match item.primitive {
            Primitive::BackdropMaterial { rect, .. } => Some(rect),
            _ => None,
        })
        .expect("palette panel");
    let selected_row = items
        .iter()
        .find_map(|item| match item.primitive {
            Primitive::RoundedQuad {
                rect,
                color,
                corner_radii,
            } if corner_radii == [12.0; 4] && color.a > 0.0 => Some(rect),
            _ => None,
        })
        .expect("selected command row");
    (panel, selected_row)
}

pub(super) fn assert_palette_title_centered(
    runtime: &Runtime,
    row: Rect,
    title: &str,
    lines: usize,
) {
    let delta = runtime.pending_delta().unwrap();
    let origin = delta
        .added
        .iter()
        .chain(&delta.modified)
        .find_map(|item| match &item.primitive {
            Primitive::Text { text, origin, .. }
                if text.as_ref() == title
                    || text
                        .strip_suffix('…')
                        .is_some_and(|prefix| !prefix.is_empty() && title.starts_with(prefix)) =>
            {
                Some(*origin)
            }
            _ => None,
        })
        .expect("command title is painted");
    let text_height = runtime.text_metrics().line_height * lines as f32;
    assert_eq!(
        origin.y,
        row.min.y + (row.size().height - text_height) / 2.0,
        "center the title without reserving a reason line: {title}",
    );
}

pub(super) fn row_surface(runtime: &Runtime, alpha: f32) -> Option<Rect> {
    let delta = runtime.pending_delta().unwrap();
    delta
        .added
        .iter()
        .chain(&delta.modified)
        .find_map(|item| match item.primitive {
            Primitive::RoundedQuad {
                rect,
                color,
                corner_radii,
            } if corner_radii == [12.0; 4] && color.a == alpha => Some(rect),
            _ => None,
        })
}

pub(super) fn row_pointer(runtime: &mut Runtime, position: Point, phase: PointerPhase) {
    runtime.dispatch(UiEvent::Pointer(PointerEvent::new(
        position,
        phase,
        PointerButton::Left,
        42,
    )));
    runtime.update(Instant::now());
}

pub(super) fn row_text(runtime: &Runtime, row: Rect, expected_color: Color) -> Vec<String> {
    let (panel, _) = palette_geometry(runtime);
    let delta = runtime.pending_delta().unwrap();
    let text: Vec<_> = delta
        .added
        .iter()
        .chain(&delta.modified)
        // Notifications can overlap the row; only inspect the palette's clipped content.
        .filter(|item| item.clips.iter().any(|clip| clip.rect() == panel))
        .filter_map(|item| match &item.primitive {
            Primitive::Text {
                text,
                origin,
                color,
                ..
            } if origin.y >= row.min.y && origin.y < row.max.y => {
                assert_eq!(*color, expected_color, "row text: {text}");
                Some(text.to_string())
            }
            _ => None,
        })
        .collect();
    assert!(!text.is_empty(), "palette row text is painted");
    text
}

pub(super) fn selected_result_viewport(runtime: &Runtime) -> (Rect, Rect) {
    let delta = runtime.pending_delta().unwrap();
    delta
        .added
        .iter()
        .chain(&delta.modified)
        .find_map(|item| match item.primitive {
            Primitive::RoundedQuad {
                rect,
                corner_radii: [12.0, 12.0, 12.0, 12.0],
                color,
            } if color.a > 0.0 => Some((
                rect,
                item.clips.last().expect("result viewport clip").rect(),
            )),
            _ => None,
        })
        .expect("selected result")
}

pub(super) fn palette_thumb(runtime: &Runtime) -> Option<Rect> {
    let delta = runtime.pending_delta().unwrap();
    delta
        .added
        .iter()
        .chain(&delta.modified)
        .find_map(|item| match item.primitive {
            Primitive::Quad {
                rect,
                color,
                corner_radius: 3.0,
            } if color.a == 0.35 || color.a == 0.6 => Some(rect),
            _ => None,
        })
}

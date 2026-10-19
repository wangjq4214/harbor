//! Application-owned command discovery and the reusable editor's palette consumer.
use super::*;
use crate::command::{
    CommandCompletion, CommandContext, CommandDescriptor, CommandSource, command_availability,
    command_registry, palette_command_supported,
};
use crate::tab_manager::{TabId, TabManager, TabSnapshot};
use harbor_widget::{
    Alignment, Column, ConstrainedBox, DecoratedBox, KeyChord,
    decoration::{Border, BorderRadius, BoxDecoration},
    layout::{BoxConstraints, Size},
    scene::primitive::Primitive,
    text::TextMetrics,
    view::PrimitiveLeaf,
    widgets::{align::Align, text_label::TextLabel},
};

#[derive(Clone)]
pub(super) struct PaletteState {
    pub generation: u64,
    pub target: Option<TabId>,
    pub input: TextEditingController,
    pub selected: usize,
    pub pending: bool,
}

// Retained by the editor's Fiber, not application state. Root replacement must
// expire a submitted selection while preserving ordinary query-state ownership.
struct PaletteEditorLifetime {
    controller: WorkspaceUiController,
    generation: u64,
}

impl Drop for PaletteEditorLifetime {
    fn drop(&mut self) {
        let pending = self
            .controller
            .store
            .state()
            .read()
            .palette
            .as_ref()
            .is_some_and(|palette| palette.generation == self.generation && palette.pending);
        if pending {
            self.controller.close_palette(self.generation);
        }
    }
}

fn candidates(query: &str) -> Vec<CommandDescriptor> {
    let query = query.to_lowercase();
    command_registry()
        .into_iter()
        .filter(|entry| {
            entry.palette_visible
                && (entry.id.to_lowercase().contains(&query)
                    || entry.label.to_lowercase().contains(&query))
        })
        .collect()
}

fn availability(
    command: AppCommand,
    target: Option<TabId>,
    snapshots: &[TabSnapshot],
    context: CommandContext,
) -> Result<(), &'static str> {
    if snapshots.iter().find(|tab| tab.active).map(|tab| tab.id) != target {
        return Err("Active terminal changed; reopen the palette");
    }
    command_availability(command, snapshots, context)
}

fn chord_keys(chord: KeyChord) -> Vec<String> {
    // KeyChord's fields are normalized by the binding resolver.
    let mut parts = Vec::new();
    if chord.modifiers.ctrl {
        parts.push("Ctrl".to_owned());
    }
    if chord.modifiers.shift {
        parts.push("Shift".to_owned());
    }
    if chord.modifiers.alt {
        parts.push("Alt".to_owned());
    }
    if chord.modifiers.meta {
        parts.push("Win".to_owned());
    }
    parts.push(match chord.key {
        harbor_widget::input::event::Key::Character(ch) => ch.to_uppercase().to_string(),
        key => format!("{key:?}"),
    });
    parts
}

// A zero-width sizing leaf keeps all rows equal without adding fake text lines.
struct PaletteRowHeight {
    shortcut_rows: usize,
}

impl PrimitiveLeaf for PaletteRowHeight {
    fn intrinsic_size(&self, constraints: BoxConstraints, metrics: &TextMetrics) -> Size {
        let shortcuts_height = self.shortcut_rows as f32 * (metrics.line_height + 2.0)
            + self.shortcut_rows.saturating_sub(1) as f32 * 2.0;
        let height = (metrics.line_height * 2.0).max(shortcuts_height);
        constraints.constrain(Size::new(0.0, height))
    }

    fn paint_primitives(&self, _rect: Rect, _metrics: &TextMetrics) -> Vec<Primitive> {
        Vec::new()
    }
}

fn shortcut_badges(
    cx: &mut BuildCx,
    bindings: &ResolvedKeybindings,
    command: AppCommand,
    foreground: Color,
) -> View {
    let keycap = BoxDecoration::new()
        .try_color(Color {
            a: 0.04,
            ..foreground
        })
        .unwrap()
        .border(
            Border::all(
                Color {
                    a: 0.25,
                    ..foreground
                },
                1.0,
            )
            .unwrap(),
        )
        .border_radius(BorderRadius::all(4.0).unwrap());
    let mut shortcuts = Column::new().cross_axis_alignment(Alignment::End).gap(2.0);
    let mut bound = false;
    for (chord, _) in bindings.bindings().filter(|(_, bound)| *bound == command) {
        bound = true;
        let mut keys = Row::new().gap(4.0);
        for key in chord_keys(chord) {
            let label = TextLabel::new(key).color(foreground).build(cx);
            keys = keys.child(
                DecoratedBox::new(keycap.clone())
                    .child(Padding::new(1.0, 2.0, 1.0, 2.0).child(label).build(cx))
                    .build(cx),
            );
        }
        shortcuts = shortcuts.child(keys.build(cx));
    }
    if !bound {
        shortcuts = shortcuts.child(TextLabel::new("Unbound").color(foreground).build(cx));
    }
    shortcuts.build(cx)
}

impl WorkspaceUiController {
    fn palette_context(&self) -> CommandContext {
        let tabs = self.tabs.state.read();
        let state = self.store.state().read();
        CommandContext {
            alternate_screen: tabs
                .active_bridge
                .as_ref()
                .is_some_and(|bridge| bridge.is_alt_screen()),
            search_open: state.search.open
                && state.search.source
                    == tabs
                        .snapshots
                        .iter()
                        .find(|tab| tab.active)
                        .map(|tab| tab.id),
        }
    }

    /// A focus loss invalidates queued selections as well as the editor's transient input.
    pub fn cancel_palette(&self) -> bool {
        let generation = self
            .store
            .state()
            .read()
            .palette
            .as_ref()
            .map(|palette| palette.generation);
        generation.is_some_and(|generation| self.close_palette(generation))
    }
    pub fn palette_is_open(&self) -> bool {
        self.store.state().read().palette.is_some()
    }

    pub fn open_palette(&self) -> FocusHandle {
        let mut state = self.store.state().read().clone();
        if state.palette.is_none() {
            state.palette_generation += 1;
            state.palette = Some(PaletteState {
                generation: state.palette_generation,
                target: self
                    .tabs
                    .state
                    .read()
                    .snapshots
                    .iter()
                    .find(|tab| tab.active)
                    .map(|tab| tab.id),
                input: TextEditingController::new(""),
                selected: 0,
                pending: false,
            });
            self.store.set_state(state);
        }
        self.palette_focus
    }

    pub fn close_palette(&self, generation: u64) -> bool {
        let mut state = self.store.state().read().clone();
        if state
            .palette
            .as_ref()
            .is_none_or(|palette| palette.generation != generation)
        {
            return false;
        }
        if let Some(palette) = state.palette.take() {
            palette.input.set_text("");
        }
        self.store.set_state(state);
        true
    }

    /// Host rechecks against its authoritative snapshots, not a rendered row.
    pub fn validate_palette_request(
        &self,
        request: AppCommandRequest,
        tabs: &TabManager,
    ) -> Result<(), &'static str> {
        let CommandSource::Palette { generation, target } = request.source else {
            return Ok(());
        };
        let state = self.store.state().read();
        let palette = state.palette.as_ref().ok_or("Palette is closed")?;
        if palette.generation != generation || !palette.pending || palette.target != target {
            return Err("Palette request expired");
        }
        if !palette_command_supported(request.command) {
            return Err("Command not yet integrated");
        }
        availability(
            request.command,
            target,
            &tabs.snapshots(),
            CommandContext {
                alternate_screen: tabs
                    .active_bridge()
                    .is_some_and(|bridge| bridge.is_alt_screen()),
                search_open: state.search.open && state.search.source == target,
            },
        )
    }

    /// One-shot admission used by the Host and window-independent integration checks.
    /// A consumed request disposes only its own editor lifetime, even when rejected.
    pub fn take_palette_request(
        &self,
        request: AppCommandRequest,
        tabs: &TabManager,
        input_blocked: bool,
    ) -> Result<(), &'static str> {
        let result = if input_blocked {
            Err("Paste confirmation is active")
        } else {
            self.validate_palette_request(request, tabs)
        };
        if let CommandSource::Palette { generation, .. } = request.source {
            self.close_palette(generation);
        }
        result
    }

    pub fn report_palette_result(&self, command: AppCommand, result: Result<(), &str>) {
        self.report_palette_completion(command, result.map(|()| CommandCompletion::Applied));
    }

    pub fn report_palette_completion(
        &self,
        command: AppCommand,
        result: Result<CommandCompletion, &str>,
    ) {
        // Success and no-change are silent: the palette closing and the action's own effect
        // (including a separate paste-confirmation window) are their feedback. Success still
        // retires a stale command failure, but never erases reload/resource failures.
        let Err(reason) = result else {
            self.publish_configuration_notice(ConfigurationNoticeSource::Command, None);
            return;
        };
        let label = command_registry()
            .into_iter()
            .find(|entry| entry.command == command)
            .map(|entry| entry.label)
            .unwrap_or("Command");
        self.publish_configuration_notice(
            ConfigurationNoticeSource::Command,
            Some(format!("{label}: {reason}")),
        );
    }

    fn palette_changed(&self, generation: u64) {
        let mut state = self.store.state().read().clone();
        if let Some(palette) = &mut state.palette
            && palette.generation == generation
            && !palette.pending
        {
            palette.selected = 0;
            self.store.set_state(state);
        }
    }

    fn palette_navigate(&self, generation: u64, down: bool) {
        let mut state = self.store.state().read().clone();
        if let Some(palette) = &mut state.palette
            && palette.generation == generation
            && !palette.pending
        {
            let count = candidates(&palette.input.text()).len();
            palette.selected = if count == 0 {
                0
            } else if down {
                (palette.selected.min(count - 1) + 1) % count
            } else {
                (palette.selected.min(count - 1) + count - 1) % count
            };
            self.store.set_state(state);
        }
    }

    fn palette_submit(&self, generation: u64) {
        let mut state = self.store.state().read().clone();
        let Some(palette) = &mut state.palette else {
            return;
        };
        if palette.generation != generation || palette.pending {
            return;
        }
        let Some(entry) = candidates(&palette.input.text())
            .get(palette.selected)
            .cloned()
        else {
            return;
        };
        let snapshots = self.tabs.state.read().snapshots.clone();
        let availability = availability(
            entry.command,
            palette.target,
            &snapshots,
            self.palette_context(),
        );
        if let Err(reason) = availability {
            self.report_palette_result(entry.command, Err(&format!("Rejected: {reason}")));
            return;
        }
        let request = AppCommandRequest {
            command: entry.command,
            focus: TabFocusPolicy::Terminal,
            source: CommandSource::Palette {
                generation,
                target: palette.target,
            },
        };
        palette.pending = true;
        self.store.set_state(state);
        self.store.dispatcher().dispatch(request);
    }
}

pub(super) fn palette_panel(
    cx: &mut BuildCx,
    controller: &WorkspaceUiController,
    bindings: &ResolvedKeybindings,
) -> View {
    let palette = controller
        .store
        .state()
        .read()
        .palette
        .clone()
        .expect("open palette");
    let generation = palette.generation;
    let changed = controller.clone();
    let navigate = controller.clone();
    let submit = controller.clone();
    let close = controller.store.dispatcher();
    let appearance = GlassAppearance::command_palette();
    let input = TextField::new(palette.input.clone())
        .decoration(appearance.input())
        .padding(6.0)
        .placeholder("Type a command label or ID…")
        .focus_handle(controller.palette_focus)
        .on_changed(move |_| changed.palette_changed(generation))
        .on_vertical_navigation(move |down| navigate.palette_navigate(generation, down))
        .on_submitted(move |_| submit.palette_submit(generation))
        .on_escape(move || {
            close.dispatch(AppCommandRequest::shortcut(
                AppCommand::CloseCommandPalette(generation),
            ))
        });
    let editor_owner = controller.clone();
    let input = move |cx: &mut BuildCx| {
        cx.use_state(|| {
            std::rc::Rc::new(PaletteEditorLifetime {
                controller: editor_owner.clone(),
                generation,
            })
        });
        input.build(cx)
    };
    let entries = candidates(&palette.input.text());
    let snapshots = controller.tabs.state.read().snapshots.clone();
    let foreground = cx.theme().button.normal.foreground;
    let mut content = Column::new()
        .cross_axis_alignment(Alignment::Stretch)
        .gap(4.0)
        .child(View::deferred(
            input.keyed(format!("palette-editor-{generation}")),
        ));
    if entries.is_empty() {
        content = content.child(
            TextLabel::new("No matching commands")
                .color(foreground)
                .build(cx),
        );
    } else {
        let shortcut_rows = candidates("")
            .iter()
            .map(|entry| {
                bindings
                    .bindings()
                    .filter(|(_, command)| *command == entry.command)
                    .count()
            })
            .max()
            .unwrap_or(1)
            .max(1);
        // Keep the selected row visible without giving keyboard focus to a second editor/list.
        let start = palette.selected.saturating_sub(4);
        for (index, entry) in entries.iter().enumerate().skip(start).take(5) {
            let reason = availability(
                entry.command,
                palette.target,
                &snapshots,
                controller.palette_context(),
            )
            .err();
            let mut labels = Column::new().cross_axis_alignment(Alignment::Stretch);
            labels = labels.child(
                TextLabel::new(entry.label)
                    .color(foreground)
                    .ellipsis()
                    .build(cx),
            );
            if let Some(reason) = reason {
                labels = labels.child(
                    TextLabel::new(reason)
                        .color(foreground)
                        .ellipsis()
                        .build(cx),
                );
            }
            let shortcuts = shortcut_badges(cx, bindings, entry.command, foreground);
            let row = Row::new()
                .cross_axis_alignment(Alignment::Center)
                .child(View::leaf(PaletteRowHeight { shortcut_rows }))
                .child(Expanded::new().child(labels.build(cx)).build(cx))
                .child(Padding::new(0.0, 0.0, 0.0, 12.0).child(shortcuts).build(cx))
                .build(cx);
            let decoration = BoxDecoration::new()
                .try_color(Color {
                    a: if index == palette.selected { 0.10 } else { 0.0 },
                    ..foreground
                })
                .unwrap()
                .border_radius(BorderRadius::all(6.0).unwrap());
            content = content.child(
                DecoratedBox::new(decoration)
                    .child(Padding::new(6.0, 8.0, 6.0, 8.0).child(row).build(cx))
                    .build(cx),
            );
        }
        content = content.child(
            TextLabel::new(format!(
                "{} / {} · ↑↓ select · Enter run · Esc close",
                palette.selected + 1,
                entries.len()
            ))
            .color(foreground)
            .build(cx),
        );
    }
    let content = Padding::all(8.0).child(content.build(cx)).build(cx);
    let card = appearance.panel(cx, content);
    Portal::new()
        .child(
            Align::new(Alignment::Center)
                .vertical_alignment(Alignment::Start)
                .insets(12.0, 12.0, 12.0, 12.0)
                .child(
                    ConstrainedBox::new()
                        .min_width(620.0)
                        .max_width(620.0)
                        .child(FocusScope::new().modal(true).child(card).build(cx))
                        .build(cx),
                )
                .build(cx),
        )
        .keyed("command-palette")
        .build(cx)
}

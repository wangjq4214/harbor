//! Application-owned command discovery and the reusable editor's palette consumer.
use super::*;
use crate::command::{
    CommandCompletion, CommandContext, CommandDescriptor, CommandSource, command_availability,
    command_registry, palette_command_supported,
};
use crate::tab_manager::{TabId, TabManager, TabSnapshot};
use harbor_config::{ProfileId, SessionProfile};
use harbor_widget::{
    Column, ConstrainedBox, ControlColors, CrossAxisAlignment, DecoratedBox, InteractiveRegion,
    KeyChord, ScrollController, ScrollView,
    decoration::{Border, BorderRadius, BoxDecoration},
    layout::{BoxConstraints, Size},
    scene::primitive::Primitive,
    text::TextMetrics,
    view::PrimitiveLeaf,
    widgets::{align::Align, text_label::TextLabel},
};
use std::collections::BTreeMap;

#[derive(Clone)]
pub(super) struct PaletteState {
    pub generation: u64,
    pub target: Option<TabId>,
    pub input: TextEditingController,
    pub selected: usize,
    level: PaletteLevel,
    /// A captured invocation is pending until its owning popup is retired.
    invocation: Option<PaletteEntry>,
    pub focus: FocusHandle,
    scroll: ScrollController,
    row_height: std::rc::Rc<std::cell::Cell<f32>>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PaletteLevel {
    Commands,
    Profiles,
}

#[derive(Clone)]
struct PaletteEntry {
    id: String,
    label: String,
    command: AppCommand,
    profile: Option<(ProfileId, SessionProfile)>,
}

fn entries(
    palette: &PaletteState,
    profiles: &BTreeMap<ProfileId, SessionProfile>,
) -> Vec<PaletteEntry> {
    let query = palette.input.text().to_lowercase();
    match palette.level {
        PaletteLevel::Commands => candidates(&query)
            .into_iter()
            .map(|entry| PaletteEntry {
                id: entry.id.into(),
                label: entry.label.into(),
                command: entry.command,
                profile: None,
            })
            .collect(),
        PaletteLevel::Profiles => profiles
            .iter()
            .filter(|(id, profile)| {
                id.as_str().to_lowercase().contains(&query)
                    || profile.name.to_lowercase().contains(&query)
            })
            .map(|(id, profile)| PaletteEntry {
                id: id.as_str().into(),
                label: profile.name.clone(),
                command: AppCommand::LaunchProfile,
                profile: Some((id.clone(), profile.clone())),
            })
            .collect(),
    }
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
            .popups
            .get::<PaletteState>()
            .is_some_and(|palette| {
                palette.generation == self.generation && palette.invocation.is_some()
            });
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
    if command == AppCommand::LaunchProfile {
        return Ok(());
    }
    command_availability(command, snapshots, context)
}

fn chord_keys(chord: KeyChord) -> Vec<String> {
    // KeyChord guarantees normalized keys independently of the binding resolver.
    let mut parts = Vec::new();
    if chord.modifiers().ctrl {
        parts.push("Ctrl".to_owned());
    }
    if chord.modifiers().shift {
        parts.push("Shift".to_owned());
    }
    if chord.modifiers().alt {
        parts.push("Alt".to_owned());
    }
    if chord.modifiers().meta {
        parts.push("Win".to_owned());
    }
    parts.push(match chord.key() {
        harbor_widget::input::event::Key::Character(ch) => ch.to_uppercase().to_string(),
        key => format!("{key:?}"),
    });
    parts
}

// A zero-width sizing leaf keeps all rows equal without adding fake text lines.
struct PaletteRowHeight {
    shortcut_rows: usize,
}

impl PaletteRowHeight {
    fn height(shortcut_rows: usize, metrics: &TextMetrics) -> f32 {
        let shortcuts_height = shortcut_rows as f32 * (metrics.line_height + 2.0)
            + shortcut_rows.saturating_sub(1) as f32 * 2.0;
        (metrics.line_height * 2.0).max(shortcuts_height)
    }
}

impl PrimitiveLeaf for PaletteRowHeight {
    fn intrinsic_size(&self, constraints: BoxConstraints, metrics: &TextMetrics) -> Size {
        constraints.constrain(Size::new(0.0, Self::height(self.shortcut_rows, metrics)))
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
    let mut shortcuts = Column::new()
        .cross_axis_alignment(CrossAxisAlignment::End)
        .gap(2.0)
        .unwrap();
    let mut bound = false;
    for (chord, _) in bindings.bindings().filter(|(_, bound)| *bound == command) {
        bound = true;
        let mut keys = Row::new().gap(4.0).unwrap();
        for key in chord_keys(chord) {
            let label = TextLabel::new(key)
                .foreground(foreground)
                .expect("valid color")
                .build(cx);
            keys = keys.push_child(
                DecoratedBox::new(keycap.clone())
                    .child(
                        Padding::new(harbor_widget::Insets {
                            top: 1.0,
                            right: 2.0,
                            bottom: 1.0,
                            left: 2.0,
                        })
                        .unwrap()
                        .child(label)
                        .build(cx),
                    )
                    .build(cx),
            );
        }
        shortcuts = shortcuts.push_child(keys.build(cx));
    }
    if !bound {
        shortcuts = shortcuts.push_child(
            TextLabel::new("Unbound")
                .foreground(foreground)
                .expect("valid color")
                .build(cx),
        );
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
            search_open: state.popups.get::<SearchUiState>().is_some_and(|search| {
                Some(search.source)
                    == tabs
                        .snapshots
                        .iter()
                        .find(|tab| tab.active)
                        .map(|tab| tab.id)
            }),
        }
    }

    /// A focus loss invalidates queued selections as well as the editor's transient input.
    pub fn cancel_palette(&self) -> bool {
        let generation = self
            .store
            .state()
            .read()
            .popups
            .get::<PaletteState>()
            .map(|palette| palette.generation);
        generation.is_some_and(|generation| self.close_palette(generation))
    }
    pub fn palette_is_open(&self) -> bool {
        self.store
            .state()
            .read()
            .popups
            .get::<PaletteState>()
            .is_some()
    }

    pub fn open_palette(&self) -> FocusHandle {
        if !self.palette_is_open() {
            return self.open_palette_level(PaletteLevel::Commands);
        }
        self.palette_focus
    }

    pub fn open_profile_chooser(&self) -> FocusHandle {
        self.open_palette_level(PaletteLevel::Profiles)
    }

    fn open_palette_level(&self, level: PaletteLevel) -> FocusHandle {
        let mut state = self.store.state().read().clone();
        let focus = state.popups.open(|generation| PaletteState {
            generation,
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
            level,
            invocation: None,
            focus: self.palette_focus,
            scroll: ScrollController::new(),
            row_height: Default::default(),
        });
        self.store.set_state(state);
        focus
    }

    /// Navigation retires the child editor; this is not restoration of a suspended popup.
    pub fn back_to_commands(&self, generation: u64) -> Option<FocusHandle> {
        let state = self.store.state().read();
        let palette = state.popups.get::<PaletteState>()?;
        if palette.generation != generation
            || palette.level != PaletteLevel::Profiles
            || palette.invocation.is_some()
        {
            return None;
        }
        drop(state);
        Some(self.open_palette_level(PaletteLevel::Commands))
    }

    /// Called only after complete configuration acceptance (including live resource effects).
    pub fn publish_profiles(&self, profiles: BTreeMap<ProfileId, SessionProfile>) {
        let mut state = self.store.state().read().clone();
        if state.profiles == profiles {
            return;
        }
        let selected = state.popups.get::<PaletteState>().and_then(|palette| {
            entries(palette, &state.profiles)
                .get(palette.selected)
                .cloned()
        });
        state.profiles = profiles;
        state.profiles_revision = state
            .profiles_revision
            .checked_add(1)
            .expect("profile catalog revision exhausted");
        let current = state.profiles.clone();
        state.popups.update::<PaletteState>(|palette| {
            if palette.level == PaletteLevel::Profiles && palette.invocation.is_none() {
                // Preserve identity, never reinterpret an old index as a different profile.
                palette.selected = selected
                    .as_ref()
                    .and_then(|old| {
                        entries(palette, &current)
                            .iter()
                            .position(|new| new.id == old.id)
                    })
                    .unwrap_or(usize::MAX);
            }
        });
        self.store.set_state(state);
    }

    /// Capture before one-shot admission closes the owning popup; never accepts direct requests.
    pub fn palette_profile_intent(
        &self,
        request: AppCommandRequest,
    ) -> Option<crate::profiles::ProfileLaunchIntent> {
        let CommandSource::Palette { generation, target } = request.source else {
            return None;
        };
        if request.command != AppCommand::LaunchProfile {
            return None;
        }
        let state = self.store.state().read();
        let palette = state.popups.get::<PaletteState>()?;
        if palette.generation != generation || palette.target != target {
            return None;
        }
        let (id, _) = palette.invocation.as_ref()?.profile.as_ref()?;
        Some(crate::profiles::ProfileLaunchIntent {
            profile: id.clone(),
            source: target,
            directory: None,
        })
    }
    pub fn close_palette(&self, generation: u64) -> bool {
        let mut state = self.store.state().read().clone();
        if state.popups.get::<PaletteState>().is_none() || !state.popups.close(generation) {
            return false;
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
        let palette = state
            .popups
            .get::<PaletteState>()
            .ok_or("Palette is closed")?;
        if palette.generation != generation
            || palette.invocation.is_none()
            || palette.target != target
        {
            return Err("Palette request expired");
        }
        if request.command == AppCommand::LaunchProfile {
            let (id, expected) = palette
                .invocation
                .as_ref()
                .and_then(|entry| entry.profile.as_ref())
                .ok_or("Profile selection expired")?;
            if palette.level != PaletteLevel::Profiles || state.profiles.get(id) != Some(expected) {
                return Err("Profile changed or was removed; select it again");
            }
            if target.is_some_and(|id| !tabs.active_live(id)) {
                return Err("Profile source is no longer active and live");
            }
        } else if !palette_command_supported(request.command) {
            return Err("Command not yet integrated");
        }
        if palette
            .invocation
            .as_ref()
            .is_none_or(|entry| entry.command != request.command)
        {
            return Err("Palette command changed");
        }
        availability(
            request.command,
            target,
            &tabs.snapshots(),
            CommandContext {
                alternate_screen: tabs
                    .active_bridge()
                    .is_some_and(|bridge| bridge.is_alt_screen()),
                search_open: state
                    .popups
                    .get::<SearchUiState>()
                    .is_some_and(|search| Some(search.source) == target),
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
            .unwrap_or(if command == AppCommand::LaunchProfile {
                "New session from profile"
            } else {
                "Command"
            });
        self.publish_configuration_notice(
            ConfigurationNoticeSource::Command,
            Some(format!("{label}: {reason}")),
        );
    }

    fn palette_changed(&self, generation: u64) {
        let mut state = self.store.state().read().clone();
        if state
            .popups
            .get::<PaletteState>()
            .is_some_and(|palette| palette.generation == generation && palette.invocation.is_none())
        {
            state.popups.update::<PaletteState>(|palette| {
                palette.selected = 0;
                palette.scroll.jump_to(0.0);
            });
            self.store.set_state(state);
        }
    }

    fn palette_navigate(&self, generation: u64, direction: harbor_widget::VerticalDirection) {
        let mut state = self.store.state().read().clone();
        let state_profiles = state.profiles.clone();
        if state
            .popups
            .get::<PaletteState>()
            .is_some_and(|palette| palette.generation == generation && palette.invocation.is_none())
        {
            state.popups.update::<PaletteState>(|palette| {
                let count = entries(palette, &state_profiles).len();
                palette.selected = if count == 0 {
                    0
                } else if palette.selected == usize::MAX {
                    if direction == harbor_widget::VerticalDirection::Down {
                        0
                    } else {
                        count - 1
                    }
                } else if direction == harbor_widget::VerticalDirection::Down {
                    (palette.selected.min(count - 1) + 1) % count
                } else {
                    (palette.selected.min(count - 1) + count - 1) % count
                };
                let height = palette.row_height.get();
                let start = palette.selected as f32 * (height + 4.0);
                palette.scroll.reveal_range(start, start + height);
            });
            self.store.set_state(state);
        }
    }

    fn palette_submit(&self, generation: u64) {
        let state = self.store.state().read().clone();
        let Some(palette) = state.popups.get::<PaletteState>() else {
            return;
        };
        if palette.input.value().composition.is_some() {
            return;
        }
        let Some(entry) = entries(palette, &state.profiles)
            .into_iter()
            .nth(palette.selected)
        else {
            return;
        };
        self.palette_submit_entry(generation, entry);
    }

    fn palette_submit_entry(&self, generation: u64, entry: PaletteEntry) {
        let mut state = self.store.state().read().clone();
        let Some(palette) = state.popups.get::<PaletteState>() else {
            return;
        };
        if palette.generation != generation || palette.invocation.is_some() {
            return;
        }
        // Mouse rows retain identity and recipe, never a potentially stale result index.
        if !entries(palette, &state.profiles).iter().any(|current| {
            current.id == entry.id
                && current.profile == entry.profile
                && current.command == entry.command
        }) {
            self.report_palette_result(
                entry.command,
                Err("Rejected: profile changed or was removed"),
            );
            return;
        }
        let snapshots = self.tabs.state.read().snapshots.clone();
        if let Err(reason) = availability(
            entry.command,
            palette.target,
            &snapshots,
            self.palette_context(),
        ) {
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
        state.popups.update::<PaletteState>(|palette| {
            palette.invocation = Some(entry);
        });
        self.store.set_state(state);
        self.store.dispatcher().dispatch(request);
    }
}

impl crate::popups::Popup for PaletteState {
    fn build(
        &self,
        cx: &mut BuildCx,
        ui: &WorkspaceUiController,
        bindings: &ResolvedKeybindings,
        _: u64,
    ) -> View {
        palette_panel(cx, ui, bindings, self)
    }
    fn focus(&self) -> FocusHandle {
        self.focus
    }
    fn close(&self) {
        self.input.set_text("");
    }
    fn blocks_shortcuts(&self) -> bool {
        true
    }
}

fn palette_panel(
    cx: &mut BuildCx,
    controller: &WorkspaceUiController,
    bindings: &ResolvedKeybindings,
    palette: &PaletteState,
) -> View {
    let generation = palette.generation;
    let changed = controller.clone();
    let navigate = controller.clone();
    let submit = controller.clone();
    let close = controller.store.dispatcher();
    let profiles_level = palette.level == PaletteLevel::Profiles;
    let appearance = GlassAppearance::command_palette();
    let input = TextField::new(palette.input.clone())
        .decoration(appearance.input())
        .padding(6.0)
        .expect("valid field padding")
        .placeholder(if palette.level == PaletteLevel::Profiles {
            "Filter profiles by name or ID…"
        } else {
            "Type a command label or ID…"
        })
        .focus_handle(controller.palette_focus)
        .on_change(move |_| changed.palette_changed(generation))
        .on_vertical_navigation(move |direction| navigate.palette_navigate(generation, direction))
        .on_submit(move |_| submit.palette_submit(generation))
        .on_escape(move || {
            close.dispatch(AppCommandRequest::shortcut(if profiles_level {
                AppCommand::BackToCommands(generation)
            } else {
                AppCommand::CloseCommandPalette(generation)
            }))
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
    let entries = entries(palette, &controller.store.state().read().profiles);
    let snapshots = controller.tabs.state.read().snapshots.clone();
    let foreground = cx.theme().button.normal.foreground;
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
    let entries_count = entries.len();
    let mut rows = Column::new()
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(4.0)
        .unwrap();
    let input = View::deferred(input.keyed(format!("palette-editor-{generation}")));
    let mut content = Column::new()
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(4.0)
        .unwrap();
    if profiles_level {
        let back = controller.store.dispatcher();
        let confirm = controller.clone();
        let style = glass_icon_button_style(cx);
        let pending = palette.invocation.is_some();
        let confirm_disabled = pending
            || entries.get(palette.selected).is_none_or(|entry| {
                availability(
                    entry.command,
                    palette.target,
                    &snapshots,
                    controller.palette_context(),
                )
                .is_err()
            });
        let revision = controller.store.state().read().profiles_revision;
        let selection = entries
            .get(palette.selected)
            .map(|entry| entry.id.clone())
            .unwrap_or_default();
        // A separate component owns button hooks; command/profile levels cannot
        // shift the parent palette's retained ScrollView hook sequence.
        let input_row = move |cx: &mut BuildCx| {
            let back = back.clone();
            let confirm = confirm.clone();
            let back = popup_icon_button(LucideIcon::ArrowLeft, style.clone())
                .focusable(false)
                .disabled(pending)
                .on_activate(move |_| {
                    back.dispatch(AppCommandRequest::popup(
                        AppCommand::BackToCommands(generation),
                        generation,
                    ))
                });
            let confirm = popup_icon_button(LucideIcon::Check, style.clone())
                .focusable(false)
                .disabled(confirm_disabled)
                .on_activate(move |_| confirm.palette_submit(generation));
            let row = Row::new()
                .cross_axis_alignment(CrossAxisAlignment::Stretch)
                .gap(4.0)
                .unwrap()
                .push_child(View::deferred(back.keyed("profile-back")))
                .push_child(Expanded::new().child(input.clone()).build(cx))
                // Retire a press if reload/navigation changes the selected recipe.
                .push_child(View::deferred(
                    confirm.keyed(format!("profile-confirm-{revision}-{selection}")),
                ))
                .build(cx);
            // Bound the stretch row to one editor line, not the whole results panel.
            // Runtime text metrics can differ from the headless fixture's 20dp line.
            ConstrainedBox::new()
                .max_height_with(|metrics| metrics.line_height + 12.0)
                .child(row)
                .build(cx)
        };
        content = content.push_child(View::deferred(
            input_row.keyed(format!("profile-input-row-{generation}")),
        ));
    } else {
        content = content.push_child(input);
    }
    if entries.is_empty() {
        rows = rows.push_child(
            TextLabel::new(if profiles_level {
                "No matching profiles"
            } else {
                "No matching commands"
            })
            .foreground(foreground)
            .expect("valid color")
            .build(cx),
        );
    } else {
        let context = controller.palette_context();
        for (index, entry) in entries.iter().enumerate() {
            let disabled = palette.invocation.is_some()
                || availability(entry.command, palette.target, &snapshots, context).is_err();
            let foreground = if disabled {
                cx.theme().button.disabled.foreground
            } else {
                foreground
            };
            let label = TextLabel::new(entry.label.clone())
                .foreground(foreground)
                .expect("valid color")
                .ellipsis()
                .build(cx);
            let shortcuts = if profiles_level {
                TextLabel::new(entry.id.clone())
                    .foreground(foreground)
                    .expect("valid color")
                    .build(cx)
            } else {
                shortcut_badges(cx, bindings, entry.command, foreground)
            };
            let row = Row::new()
                .cross_axis_alignment(CrossAxisAlignment::Center)
                .push_child(View::leaf(PaletteRowHeight { shortcut_rows }))
                .push_child(Expanded::new().child(label).build(cx))
                .push_child(
                    Padding::new(harbor_widget::Insets {
                        top: 0.0,
                        right: 0.0,
                        bottom: 0.0,
                        left: 12.0,
                    })
                    .unwrap()
                    .child(shortcuts)
                    .build(cx),
                )
                .build(cx);
            let activate = controller.clone();
            let selected_entry = entry.clone();
            let mut style = cx.theme().button.clone();
            let transparent = ControlColors {
                background: Color::TRANSPARENT,
                border: Color::TRANSPARENT,
                foreground,
            };
            style.min_height = 0.0;
            style.horizontal_padding = 0.0;
            style.border_width = 0.0;
            style.normal = transparent;
            style.hovered = transparent;
            style.pressed = transparent;
            style.selected = transparent;
            style.disabled = transparent;
            let region = InteractiveRegion::new()
                .style(style)
                .expect("valid control style")
                .focusable(false)
                .selected(index == palette.selected)
                .disabled(disabled)
                .on_activate(move |_| {
                    activate.palette_submit_entry(generation, selected_entry.clone())
                })
                .child_with(move |cx, state| {
                    let content = Padding::new(harbor_widget::Insets {
                        top: 6.0,
                        right: 8.0,
                        bottom: 6.0,
                        left: 8.0,
                    })
                    .unwrap()
                    .child(row.clone())
                    .build(cx);
                    match GlassAppearance::command_palette_row(foreground, state) {
                        Some(glass) => glass.panel(cx, content),
                        None => content,
                    }
                });
            rows = rows.push_child(View::deferred(region.keyed(if profiles_level {
                format!(
                    "palette-row-{generation}-{}-{}",
                    entry.id,
                    controller.store.state().read().profiles_revision
                )
            } else {
                format!("palette-row-{generation}-{}", entry.id)
            })));
        }
        let row_height = palette.row_height.clone();
        // Uniform rows let committed content geometry determine the exact row height.
        let rows = harbor_widget::LayoutObserver::new(move |rect| {
            row_height.set((rect.size().height + 4.0) / entries_count as f32 - 4.0);
        })
        .child(rows.build(cx))
        .build(cx);
        // Reserve the 10dp overlay hit target plus a 4dp gap outside row glass.
        // Keep ScrollView's geometry and scrolling behavior unchanged.
        let rows = Padding::new(harbor_widget::Insets {
            top: 0.0,
            right: 14.0,
            bottom: 0.0,
            left: 0.0,
        })
        .unwrap()
        .child(rows)
        .build(cx);
        content = content.push_child(
            Expanded::new()
                .child(
                    ScrollView::new()
                        .controller(palette.scroll.clone())
                        .child(rows)
                        .build(cx),
                )
                .build(cx),
        );
        content = content.push_child(
            TextLabel::new(format!(
                "{} / {} · ↑↓ select · Enter / click run · Esc {}",
                if palette.selected < entries.len() {
                    palette.selected + 1
                } else {
                    0
                },
                entries.len(),
                if profiles_level { "back" } else { "close" }
            ))
            .foreground(foreground)
            .expect("valid color")
            .build(cx),
        );
    }
    if entries.is_empty() {
        content = content.push_child(
            Expanded::new()
                .child(
                    ScrollView::new()
                        .controller(palette.scroll.clone())
                        .child(rows.build(cx))
                        .build(cx),
                )
                .build(cx),
        );
    }
    let visible = entries.len().min(5);
    let content = ConstrainedBox::new()
        .max_height_with(move |metrics| {
            let editor = metrics.line_height + 12.0;
            if visible == 0 {
                editor + 4.0 + metrics.line_height
            } else {
                editor
                    + 8.0
                    + metrics.line_height
                    + visible as f32 * (PaletteRowHeight::height(shortcut_rows, metrics) + 12.0)
                    + visible.saturating_sub(1) as f32 * 4.0
            }
        })
        .child(content.build(cx))
        .build(cx);
    let content = Padding::all(8.0).unwrap().child(content).build(cx);
    let card = appearance.panel(cx, content);
    Portal::new()
        .child(
            Align::new(harbor_widget::Alignment2D::new(
                harbor_widget::PositionAlignment::Center,
                harbor_widget::PositionAlignment::Start,
            ))
            .insets(harbor_widget::Insets {
                top: 12.0,
                right: 12.0,
                bottom: 12.0,
                left: 12.0,
            })
            .unwrap()
            .child(
                harbor_widget::SizedBox::width(620.0)
                    .unwrap()
                    .child(FocusScope::new().modal(true).child(card).build(cx))
                    .build(cx),
            )
            .build(cx),
        )
        .keyed("command-palette")
        .build(cx)
}

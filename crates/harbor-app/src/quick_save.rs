//! Application-owned input-only naming and in-window review. No filesystem effects.
use super::*;
use crate::appearance::design_system::{self as design, ControlRole, TextRole};
#[path = "quick_save_project.rs"]
mod project_review;
use harbor_config::{SessionProfile, ShellSettings};
use harbor_widget::input::event::{Key, Modifiers};
use harbor_widget::widgets::text_label::TextLabel;
use harbor_widget::{
    Button, Checkbox, Column, ConstrainedBox, DecoratedBox, KeyChord, ScrollView, SegmentedControl,
};
use project_review::ProjectReview;
use std::collections::BTreeSet;

/// Stable Host-captured source. ShellSettings deliberately redacts environment Debug output.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QuickSaveDraft {
    pub source: crate::tab_manager::TabId,
    pub launch: ShellSettings,
    pub project: Option<harbor_config::ProjectRecipe>,
}
#[derive(Clone, Debug)]
pub struct QuickSaveSubmission {
    pub generation: u64,
    pub profile: SessionProfile,
}
#[derive(Clone)]
struct QuickSaveState {
    draft: QuickSaveDraft,
    project: ProjectReview,
    /// Frozen at final approval; its presence also marks a pending submission.
    approved_project: Option<harbor_config::ProjectRecipe>,
    input: TextEditingController,
    review_name: Option<String>,
    details_open: bool,
    selected: BTreeSet<String>,
    acknowledged: bool,
    issued: bool,
    error: Option<String>,
    review_scroll: harbor_widget::ScrollController,
    focus: FocusHandle,
}
// A queued approval belongs to the mounted review, not merely retained Store data.
struct QuickSaveLifetime {
    ui: WorkspaceUiController,
    generation: u64,
}
impl Drop for QuickSaveLifetime {
    fn drop(&mut self) {
        let state = self.ui.store.state().read();
        let retire = state.popups.is_current(self.generation)
            && state
                .popups
                .get::<QuickSaveState>()
                .is_some_and(|save| save.pending() && !save.issued);
        drop(state);
        if retire {
            self.ui.cancel_quick_save(AppCommandRequest::popup(
                AppCommand::CancelQuickSave,
                self.generation,
            ));
        }
    }
}

fn name_error(
    name: &str,
    profiles: &std::collections::BTreeMap<harbor_config::ProfileId, SessionProfile>,
) -> Option<&'static str> {
    let name = name.trim();
    if name.is_empty() || name.chars().any(char::is_control) {
        Some("Enter a preset name (no control characters)")
    } else if profiles.values().any(|profile| profile.name == name) {
        Some("A preset with this name already exists")
    } else {
        None
    }
}

impl WorkspaceUiController {
    pub fn open_quick_save(&self, draft: QuickSaveDraft) -> FocusHandle {
        let mut state = self.store.state().read().clone();
        state
            .notices
            .publish(ConfigurationNoticeSource::QuickSave, None);
        let focus = state.popups.open(|_| QuickSaveState {
            project: ProjectReview::new(draft.project.clone()),
            approved_project: None,
            draft,
            input: TextEditingController::new(""),
            review_name: None,
            details_open: false,
            selected: BTreeSet::new(),
            acknowledged: false,
            issued: false,
            error: None,
            review_scroll: harbor_widget::ScrollController::new(),
            focus: FocusHandle::new(),
        });
        self.store.set_state(state);
        focus
    }
    pub fn quick_save_is_open(&self) -> bool {
        self.store
            .state()
            .read()
            .popups
            .get::<QuickSaveState>()
            .is_some()
    }
    /// Retires the editor/controller on every step; old clipboard/IME cannot follow it.
    pub fn navigate_quick_save(&self, request: AppCommandRequest) -> Option<FocusHandle> {
        let crate::command::CommandSource::Popup { generation } = request.source else {
            return None;
        };
        let mut state = self.store.state().read().clone();
        if !state.popups.is_current(generation) {
            return None;
        }
        let mut save = state.popups.get::<QuickSaveState>()?.clone();
        if save.pending() {
            return None;
        }
        match request.command {
            AppCommand::AdvanceQuickSave if save.review_name.is_none() => {
                if save.input.value().composition.is_some() {
                    return None;
                }
                if let Some(error) = name_error(&save.input.text(), &state.profiles) {
                    state
                        .notices
                        .publish(ConfigurationNoticeSource::QuickSave, Some(error.into()));
                    self.store.set_state(state);
                    return None;
                }
                save.review_name = Some(save.input.text().trim().into());
                // A sink receives initial review focus, never the final Save button.
                save.focus = FocusHandle::new();
            }
            AppCommand::EditQuickSave if save.review_name.is_some() => {
                save.input = TextEditingController::new(save.review_name.take().unwrap());
                save.focus = FocusHandle::new();
            }
            _ => return None,
        }
        state
            .notices
            .publish(ConfigurationNoticeSource::QuickSave, None);
        save.error = None;
        save.review_scroll.jump_to(0.0);
        save.project.retire_editors();
        let focus = state.popups.open(|_| save);
        self.store.set_state(state);
        Some(focus)
    }
    pub fn cancel_quick_save(&self, request: AppCommandRequest) -> bool {
        let crate::command::CommandSource::Popup { generation } = request.source else {
            return false;
        };
        let mut state = self.store.state().read().clone();
        if state.popups.get::<QuickSaveState>().is_none() || !state.popups.close(generation) {
            return false;
        }
        state
            .notices
            .publish(ConfigurationNoticeSource::QuickSave, None);
        self.store.set_state(state);
        true
    }
    pub fn cancel_quick_save_on_focus_loss(&self) -> bool {
        let Some(generation) = self.store.state().read().popups.generation() else {
            return false;
        };
        self.cancel_quick_save(AppCommandRequest::popup(
            AppCommand::CancelQuickSave,
            generation,
        ))
    }
    fn update_quick_save(&self, generation: u64, update: impl FnOnce(&mut QuickSaveState)) {
        let mut state = self.store.state().read().clone();
        if !state.popups.is_current(generation) {
            return;
        }
        state.popups.update::<QuickSaveState>(|save| {
            if !save.pending() {
                update(save);
            }
        });
        self.store.set_state(state);
    }
    fn queue_quick_save(&self, generation: u64) {
        let mut queued = false;
        self.update_quick_save(generation, |save| {
            if save.review_name.is_some()
                && let Ok(project) = save.project.recipe()
                && save.overrides_approved()
            {
                save.approved_project = Some(project);
                queued = true;
            }
        });
        if queued {
            self.store.dispatcher().dispatch(AppCommandRequest::popup(
                AppCommand::CommitQuickSave,
                generation,
            ));
        }
    }
    /// One-shot Host admission. Direct or stale queued requests carry no authority.
    pub fn take_quick_save_submission(
        &self,
        request: AppCommandRequest,
    ) -> Option<QuickSaveSubmission> {
        let crate::command::CommandSource::Popup { generation } = request.source else {
            return None;
        };
        let mut state = self.store.state().read().clone();
        if request.command != AppCommand::CommitQuickSave || !state.popups.is_current(generation) {
            return None;
        }
        let save = state.popups.get::<QuickSaveState>()?;
        if save.issued || !save.overrides_approved() {
            return None;
        }
        let name = save.review_name.clone()?;
        let project = save.approved_project.clone()?;
        let mut launch = save.draft.launch.clone();
        launch.program = Some(project.environment.program().into());
        launch.args.clear();
        launch.directory = None;
        launch
            .environment
            .retain(|key, _| save.selected.contains(key));
        let submission = QuickSaveSubmission {
            generation,
            profile: SessionProfile {
                name,
                launch,
                project: Some(project),
            },
        };
        // Keep the layer blocked until Host completion. Duplicate delivery is denied.
        state
            .popups
            .update::<QuickSaveState>(|save| save.issued = true);
        self.store.set_state(state);
        Some(submission)
    }
    pub fn finish_quick_save(
        &self,
        generation: u64,
        result: Result<(), String>,
    ) -> Option<FocusHandle> {
        let mut state = self.store.state().read().clone();
        if !state.popups.is_current(generation) || state.popups.get::<QuickSaveState>().is_none() {
            return None;
        }
        let focus = match result {
            Ok(()) => {
                state.popups.close(generation);
                self.terminal_focus
            }
            Err(error) => {
                state.popups.update::<QuickSaveState>(|save| {
                    save.approved_project = None;
                    save.issued = false;
                    save.error = Some(error);
                    save.review_scroll.jump_to(f32::INFINITY);
                });
                state.popups.get::<QuickSaveState>().unwrap().focus
            }
        };
        self.store.set_state(state);
        Some(focus)
    }
}
/// Keep control characters inert without exposing Rust's Debug formatting.
fn display_value(value: &str) -> String {
    value
        .chars()
        .map(|ch| match ch {
            '\\' | '"' | '\'' => ch.to_string(),
            _ => ch.escape_debug().to_string(),
        })
        .collect()
}

/// Borderless, selectable text keeps long captured values accessible by keyboard.
fn review_line(text: String, key: &'static str) -> View {
    View::deferred(
        (move |cx: &mut BuildCx| {
            let input = cx.use_state(|| TextEditingController::new(text.clone()));
            if input.read().text() != text {
                input.read().set_text(text.clone());
            }
            harbor_widget::EditableText::new(input.read().clone())
                .read_only(true)
                .foreground(design::FOREGROUND)
                .unwrap()
                .build(cx)
        })
        .keyed(key),
    )
}

/// Explanations use UI text geometry; captured values remain selectable editors.
fn review_message(text: impl Into<String>, key: &'static str, warning: bool) -> View {
    View::deferred(
        design::text(
            text,
            if warning {
                TextRole::Warning
            } else {
                TextRole::Supporting
            },
        )
        .keyed(key),
    )
}

fn review_label(text: &str) -> TextLabel {
    design::text(text, TextRole::Label)
}

fn review_divider(key: &'static str) -> View {
    View::deferred(
        harbor_widget::Separator::horizontal()
            .color(design::DIVIDER)
            .unwrap()
            .keyed(key),
    )
}

/// A label stays closer to its field than to the next field or section.
fn review_field(cx: &mut BuildCx, label: &str, field: View) -> View {
    Column::new()
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(design::LABEL_GAP)
        .unwrap()
        .push_child(review_label(label).build(cx))
        .push_child(field)
        // Conditional target fields must not retire the neighboring directory editor.
        .keyed(format!("review-field-{label}"))
        .build(cx)
}

/// Product grouping stays in the application; fields retain their own surfaces.
fn review_block(cx: &mut BuildCx, title: &str, heading_key: &'static str) -> Column {
    Column::new()
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(design::FIELD_GAP)
        .unwrap()
        .push_child(
            design::text(title, TextRole::Section)
                .keyed(heading_key)
                .build(cx),
        )
}

impl crate::popups::Popup for QuickSaveState {
    fn focus(&self) -> FocusHandle {
        self.focus
    }
    fn close(&self) {
        self.input.set_text("");
        self.project.clear_editors();
    }
    fn blocks_shortcuts(&self) -> bool {
        true
    }
    fn notification_inset(&self) -> f32 {
        if self.review_name.is_none() {
            // Keep the existing top-right notification below the compact naming bar.
            12.0 + 2.0 * design::INPUT_PADDING + design::CONTROL_HEIGHT
        } else {
            0.0
        }
    }
    fn build(
        &self,
        cx: &mut BuildCx,
        ui: &WorkspaceUiController,
        _: &ResolvedKeybindings,
        generation: u64,
    ) -> View {
        let save = self.clone();
        let ui = ui.clone();
        harbor_widget::ThemeProvider::new(design::theme(cx.theme()))
            .expect("valid application theme")
            .child(
                (move |cx: &mut BuildCx| save.panel(cx, &ui, generation))
                    .keyed(format!("quick-save-owner-{generation}")),
            )
            .build(cx)
    }
}
impl QuickSaveState {
    fn pending(&self) -> bool {
        self.approved_project.is_some()
    }

    fn overrides_approved(&self) -> bool {
        self.selected.is_empty() || self.acknowledged
    }

    fn panel(&self, cx: &mut BuildCx, ui: &WorkspaceUiController, generation: u64) -> View {
        cx.use_state(|| {
            std::rc::Rc::new(QuickSaveLifetime {
                ui: ui.clone(),
                generation,
            })
        });
        let reviewing = self.review_name.is_some();
        let appearance = if reviewing {
            GlassAppearance::command_palette()
        } else {
            GlassAppearance::default()
        };
        let dispatch = ui.store.dispatcher();
        let send = move |command| dispatch.dispatch(AppCommandRequest::popup(command, generation));
        let mut content = Column::new()
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .gap(if reviewing {
                design::SECTION_GAP
            } else {
                design::FIELD_GAP
            })
            .unwrap();
        let mut review_header = None;
        let mut review_footer = None;
        if let Some(name) = &self.review_name {
            let back = send.clone();
            let header = Column::new()
                .cross_axis_alignment(CrossAxisAlignment::Stretch)
                .gap(design::LABEL_GAP)
                .unwrap()
                .push_child(
                    Focus::new()
                        .focus_handle(self.focus)
                        .child(design::text("Save preset", TextRole::Title))
                        .build(cx),
                )
                .push_child(
                    Row::new()
                        .cross_axis_alignment(CrossAxisAlignment::Center)
                        .gap(design::LABEL_GAP)
                        .unwrap()
                        .push_child(
                            Expanded::new().child(review_line(display_value(name), "review-name")),
                        )
                        .push_child(
                            Button::text("Edit name")
                                .style(design::control(cx, ControlRole::Quiet))
                                .unwrap()
                                .disabled(self.pending())
                                .on_click(move |_| back(AppCommand::EditQuickSave))
                                .keyed("quick-save-back"),
                        )
                        .build(cx),
                )
                .keyed("review-header")
                .build(cx);
            review_header = Some(header);
            content = content.push_child(self.project.panel(cx, ui, generation, self.pending()));

            // Opt-in choices and their independent acknowledgement never live in disclosure.
            let mut overrides = if self.draft.launch.environment.is_empty() {
                Column::new().cross_axis_alignment(CrossAxisAlignment::Stretch)
            } else {
                review_block(cx, "Environment variables", "overrides-heading")
            };
            overrides = overrides.push_child(review_message(
                if self.draft.launch.environment.is_empty() {
                    "No environment overrides will be saved.".into()
                } else {
                    format!("{} of {} overrides selected. Values stay hidden; unselected entries are excluded.",
                        self.selected.len(), self.draft.launch.environment.len())
                },
                "review-overrides", false,
            ));
            for (index, key) in self.draft.launch.environment.keys().enumerate() {
                let selected = self.selected.contains(key);
                let key = key.clone();
                let change = ui.clone();
                overrides = overrides.push_child(View::deferred(
                    Checkbox::new(display_value(&key), selected)
                        .style(design::control(cx, ControlRole::Checkbox))
                        .unwrap()
                        .disabled(self.pending())
                        .on_change(move |checked| {
                            change.update_quick_save(generation, |save| {
                                if checked {
                                    save.selected.insert(key.clone());
                                } else {
                                    save.selected.remove(&key);
                                }
                                save.acknowledged = false;
                            });
                        })
                        .keyed(format!("environment-{index}")),
                ));
            }
            if !self.selected.is_empty() {
                let change = ui.clone();
                overrides = overrides.push_child(View::deferred(
                    Checkbox::new(
                        "Selected variables contain no passwords or tokens",
                        self.acknowledged,
                    )
                    .style(design::control(cx, ControlRole::Checkbox))
                    .unwrap()
                    .disabled(self.pending())
                    .on_change(move |checked| {
                        change.update_quick_save(generation, |save| save.acknowledged = checked)
                    })
                    .keyed("quick-save-ack"),
                ));
                if !self.acknowledged {
                    overrides = overrides.push_child(review_message(
                        "Check the selected variables and confirm above before saving.",
                        "override-error",
                        true,
                    ));
                }
            }

            content = content.push_child(overrides.build(cx));
            let mut details = Column::new()
                .cross_axis_alignment(CrossAxisAlignment::Stretch)
                .gap(design::FIELD_GAP)
                .unwrap();
            let suspicious_source =
                harbor_config::project::suspected_credentials(&self.draft.launch.args);
            if suspicious_source {
                details = details.push_child(review_message(
                    "Source startup arguments are hidden because they may contain credentials. They are not saved or replayed.",
                    "source-secret-warning", true,
                ));
            }
            let change = ui.clone();
            details = details.push_child(
                Row::new()
                    .push_child(View::deferred(
                        Button::icon(
                            if self.details_open {
                                LucideIcon::ChevronUp
                            } else {
                                LucideIcon::ChevronDown
                            },
                            "Source details",
                        )
                        .style(design::control(cx, ControlRole::Quiet))
                        .unwrap()
                        .disabled(self.pending())
                        .on_click(move |_| {
                            change.update_quick_save(generation, |save| {
                                save.details_open = !save.details_open
                            })
                        })
                        .keyed("quick-save-details"),
                    ))
                    .build(cx),
            );
            if self.details_open {
                let arguments = if suspicious_source {
                    "[hidden: suspected credentials]".into()
                } else if self.draft.launch.args.is_empty() {
                    "None".into()
                } else {
                    self.draft
                        .launch
                        .args
                        .iter()
                        .map(|arg| {
                            if arg.is_empty()
                                || arg.chars().any(|ch| ch.is_whitespace() || ch == '"')
                            {
                                format!("{arg:?}")
                            } else {
                                display_value(arg)
                            }
                        })
                        .collect::<Vec<_>>()
                        .join(" ")
                };
                let directory = self
                    .draft
                    .launch
                    .directory
                    .as_ref()
                    .map(|path| display_value(&path.to_string_lossy()))
                    .unwrap_or_else(|| "Unknown".into());
                details = details
                    .push_child(review_line(format!("Source program (provenance): {}",
                        display_value(self.draft.launch.program.as_deref().unwrap_or("Default shell"))), "review-program"))
                    .push_child(review_line(format!("Source arguments (not replayed): {arguments}"), "review-args"))
                    .push_child(review_line(format!("Initial directory (not current-directory evidence): {directory}"), "review-directory"))
                    .push_child(review_message(
                        "The preset uses its selected environment, project directory and included agent, not the source shell's startup arguments. History, nested environments and virtual-environment activation are not restored.",
                        "project-limitations", false,
                    ));
            }

            content = content.push_child(details.build(cx));
            let mut actions = Column::new()
                .cross_axis_alignment(CrossAxisAlignment::Stretch)
                .gap(8.0)
                .unwrap()
                .push_child(review_divider("actions-divider"));
            if let Some(error) = &self.error {
                // Keep complete errors next to the footer, but in the scrolling body:
                // a long filesystem error must not consume the fixed controls' budget.
                content = content.push_child(review_message(error.clone(), "save-error", true));
            }
            let cancel = send.clone();
            let commit = ui.clone();
            actions = actions.push_child(
                Row::new()
                    .wrap(true)
                    .main_axis_alignment(harbor_widget::MainAxisAlignment::End)
                    .gap(8.0)
                    .unwrap()
                    .push_child(View::deferred(
                        Button::text("Cancel")
                            .style(design::control(cx, ControlRole::Quiet))
                            .unwrap()
                            .disabled(self.pending())
                            .on_click(move |_| cancel(AppCommand::CancelQuickSave))
                            .keyed("quick-save-cancel"),
                    ))
                    .push_child(View::deferred(
                        Button::text(if self.pending() { "Saving…" } else { "Save" })
                            .style(design::control(cx, ControlRole::Primary))
                            .unwrap()
                            .disabled(
                                self.pending()
                                    || self.project.recipe().is_err()
                                    || !self.overrides_approved(),
                            )
                            .on_click(move |_| commit.queue_quick_save(generation))
                            .keyed("quick-save-final"),
                    ))
                    .build(cx),
            );
            review_footer = Some(actions.keyed("review-footer").build(cx));
        } else {
            let changed = ui.clone();
            let submit = send.clone();
            let cancel = send.clone();
            let input = TextField::new(self.input.clone())
                .decoration(appearance.input())
                .foreground(design::FOREGROUND)
                .unwrap()
                .padding(6.0)
                .unwrap()
                .placeholder("Preset name…")
                .focus_handle(self.focus)
                .on_change(move |_| {
                    if changed.store.state().read().popups.is_current(generation) {
                        changed.publish_configuration_notice(
                            ConfigurationNoticeSource::QuickSave,
                            None,
                        );
                    }
                })
                .on_submit(move |_| submit(AppCommand::AdvanceQuickSave))
                .on_escape(move || cancel(AppCommand::CancelQuickSave));
            let advance = send.clone();
            let button = popup_icon_button(
                LucideIcon::Check,
                crate::appearance::glass_icon_button_style(cx),
            )
            .disabled(self.pending())
            .on_activate(move |_| advance(AppCommand::AdvanceQuickSave));
            content = content.push_child(
                ConstrainedBox::new()
                    .max_height_with(|metrics| metrics.line_height + 12.0)
                    .child(
                        Row::new()
                            .cross_axis_alignment(CrossAxisAlignment::Stretch)
                            .gap(4.0)
                            .unwrap()
                            .push_child(
                                Expanded::new()
                                    .child(View::deferred(
                                        input.keyed(format!("save-editor-{generation}")),
                                    ))
                                    .build(cx),
                            )
                            .push_child(View::deferred(button.keyed("save-advance")))
                            .build(cx),
                    )
                    .build(cx),
            );
        }
        let cancel = send.clone();
        let actions = ActionHandler::new(move |command| {
            cancel(command);
            ActionOutcome::Consumed
        });
        let content = if let (Some(header), Some(footer)) = (review_header, review_footer) {
            let body = ScrollView::new()
                .controller(self.review_scroll.clone())
                .shrink_wrap(true)
                .child(
                    Padding::new(Insets {
                        right: 10.0,
                        ..Insets::default()
                    })
                    .unwrap()
                    .child(content)
                    .build(cx),
                )
                .keyed("review-body")
                .build(cx);
            Padding::new(Insets {
                left: design::PANEL_PADDING,
                right: design::PANEL_PADDING,
                top: design::INPUT_PADDING,
                bottom: design::INPUT_PADDING,
            })
            .unwrap()
            .child(
                Column::new()
                    .shrink_wrap(true)
                    .cross_axis_alignment(CrossAxisAlignment::Stretch)
                    .gap(design::LABEL_GAP)
                    .unwrap()
                    .push_child(header)
                    .push_child(harbor_widget::Flexible::new().child(body))
                    .push_child(footer)
                    .build(cx),
            )
            .build(cx)
        } else {
            Padding::all(design::INPUT_PADDING)
                .unwrap()
                .child(ScrollView::new().shrink_wrap(true).child(content).build(cx))
                .build(cx)
        };
        let card = appearance.panel(cx, content);
        let card = actions
            .child(
                ShortcutMap::new()
                    .bind(
                        KeyChord::new(Key::Escape, Modifiers::default()),
                        AppCommand::CancelQuickSave,
                    )
                    .child(card),
            )
            .build(cx);
        let aligned = Align::new(Alignment2D::new(
            harbor_widget::PositionAlignment::Center,
            harbor_widget::PositionAlignment::Start,
        ))
        .insets(Insets::all(12.0))
        .unwrap()
        .child(
            ConstrainedBox::new()
                .max_width(if reviewing { 600.0 } else { 620.0 })
                .max_height(720.0)
                .child(FocusScope::new().modal(true).child(card).build(cx))
                .build(cx),
        )
        .build(cx);
        Portal::new()
            .child(aligned)
            .keyed(format!("quick-save-{generation}"))
            .build(cx)
    }
}

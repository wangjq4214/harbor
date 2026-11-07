//! Review-only project correction. Editors never directly authorize a save.
use super::*;
use harbor_config::{ProjectEnvironment, ProjectRecipe, project::suspected_credentials};

#[derive(Clone)]
pub(super) struct ProjectReview {
    kind: String,
    target: TextEditingController,
    directory: TextEditingController,
    applied: Option<ProjectRecipe>,
    include_agent: bool,
    checked_arguments: bool,
    changed: bool,
    correction_error: Option<String>,
}
impl ProjectReview {
    pub(super) fn new(project: Option<ProjectRecipe>) -> Self {
        Self {
            kind: project
                .as_ref()
                .map_or("pwsh", |p| p.environment.kind())
                .into(),
            target: TextEditingController::new(
                project.as_ref().map_or("", |p| p.environment.target()),
            ),
            directory: TextEditingController::new(
                project.as_ref().map_or("", |p| p.directory.as_str()),
            ),
            applied: project,
            include_agent: true,
            checked_arguments: false,
            changed: false,
            correction_error: None,
        }
    }
    pub(super) fn retire_editors(&mut self) {
        self.target = TextEditingController::new(self.target.text());
        self.directory = TextEditingController::new(self.directory.text());
    }
    pub(super) fn clear_editors(&self) {
        self.target.set_text("");
        self.directory.set_text("");
    }
    fn pending_correction(&self) -> bool {
        self.applied.as_ref().is_none_or(|p| {
            p.environment.kind() != self.kind
                || p.environment.target() != self.target.text()
                || p.directory != self.directory.text()
        })
    }
    pub(super) fn recipe(&self) -> Result<ProjectRecipe, &'static str> {
        if self.target.value().composition.is_some() || self.directory.value().composition.is_some()
        {
            return Err("Finish composing the context correction first");
        }
        if self.pending_correction() {
            return Err(
                "Unknown or edited context: apply the environment/directory correction first",
            );
        }
        let mut recipe = self
            .applied
            .clone()
            .ok_or("Current environment/directory is unknown")?;
        if !self.include_agent {
            recipe.agent = None;
        }
        recipe.validate()?;
        if recipe.agent.is_some() && !self.checked_arguments {
            return Err("Review agent arguments and confirm they contain no credentials");
        }
        Ok(recipe)
    }
    fn apply(&mut self) -> Result<(), &'static str> {
        if self.target.value().composition.is_some() || self.directory.value().composition.is_some()
        {
            return Err("Finish composing the correction first");
        }
        let environment = ProjectEnvironment::new(&self.kind, &self.target.text()).ok_or(
            "Enter a valid environment target (SSH config alias/user@host or WSL distribution)",
        )?;
        let mut recipe = ProjectRecipe {
            environment,
            directory: self.directory.text(),
            agent: None,
        };
        recipe.validate()?;
        // An environment correction never borrows a different environment's agent record.
        if let Some(previous) = &self.applied
            && previous.environment == recipe.environment
        {
            recipe.agent = previous.agent.clone();
        }
        self.applied = Some(recipe);
        self.changed = true;
        self.checked_arguments = false;
        Ok(())
    }
    pub(super) fn panel(
        &self,
        cx: &mut BuildCx,
        ui: &WorkspaceUiController,
        generation: u64,
        pending: bool,
    ) -> View {
        let appearance = GlassAppearance::command_palette();
        let mut sections = Column::new()
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .gap(18.0)
            .unwrap();
        let mut column = review_block(cx, "Project and execution environment", "project-divider");
        column = column.push_child(review_message(
            if self.applied.is_none() {
                "Environment / current directory unknown. Correct the context below; initial cwd is not current-directory evidence."
            } else if self.changed {
                "Context explicitly corrected, not automatically recognized. Changing environments clears the captured agent."
            } else {
                "Captured shell metadata — review the environment and current project directory."
            }, "project-status", self.applied.is_none(),
        ));
        let change = ui.clone();
        column = column
            .push_child(review_label("Execution environment").build(cx))
            .push_child(View::deferred(
                SegmentedControl::new(
                    [
                        ("pwsh".into(), "Local pwsh".into()),
                        ("wsl".into(), "WSL bash".into()),
                        ("ssh".into(), "Direct SSH bash".into()),
                    ],
                    self.kind.clone(),
                )
                .style(quick_save_button_style(cx, false))
                .unwrap()
                .disabled(pending)
                .on_change(move |kind| {
                    change.update_quick_save(generation, |s| {
                        if s.project.kind != kind {
                            s.project.kind = kind;
                            s.project.target.set_text("");
                            s.project.correction_error = None;
                        }
                    })
                })
                .keyed("project-environment"),
            ));
        if self.kind != "pwsh" {
            let target_change = ui.clone();
            let (label, example) = if self.kind == "wsl" {
                ("WSL distribution", "e.g. Ubuntu")
            } else {
                ("SSH destination", "e.g. user@host or a config alias")
            };
            column = column
                .push_child(review_label(label).build(cx))
                .push_child(View::deferred(
                    TextField::new(self.target.clone())
                        .decoration(appearance.input())
                        .disabled(pending)
                        .placeholder(example)
                        .on_change(move |_| target_change.update_quick_save(generation, |_| {}))
                        .keyed("project-target"),
                ));
        }
        let directory_change = ui.clone();
        column = column
            .push_child(review_label("Project directory").build(cx))
            .push_child(View::deferred(
                TextField::new(self.directory.clone())
                    .decoration(appearance.input())
                    .disabled(pending)
                    .placeholder(if self.kind == "pwsh" {
                        "e.g. C:\\work\\project"
                    } else {
                        "e.g. /home/user/project"
                    })
                    .on_change(move |_| directory_change.update_quick_save(generation, |_| {}))
                    .keyed("project-directory"),
            ))
            .push_child(review_message(
                match self.kind.as_str() {
                    "wsl" => {
                        "Absolute path inside the selected WSL distribution, not a Windows path."
                    }
                    "ssh" => "Absolute path on the SSH destination, not a local Windows path.",
                    _ => "Absolute path on this computer for local PowerShell.",
                },
                "project-directory-help",
                false,
            ));
        let change = ui.clone();
        column = column.push_child(
            Row::new()
                .push_child(View::deferred(
                    Button::text("Apply context correction")
                        .style(quick_save_button_style(cx, false))
                        .unwrap()
                        .disabled(pending)
                        .on_click(move |_| {
                            change.update_quick_save(generation, |s| {
                                s.project.correction_error =
                                    s.project.apply().err().map(str::to_owned);
                            })
                        })
                        .keyed("project-apply"),
                ))
                .build(cx),
        );
        if let Some(error) = &self.correction_error {
            column = column.push_child(review_message(error.clone(), "context-error", true));
        }
        if self.pending_correction() {
            column = column.push_child(review_message(
                "Unknown or edited context: apply the environment/directory correction first.",
                "context-pending",
                true,
            ));
        }
        let context_blocked = self.pending_correction()
            || self.target.value().composition.is_some()
            || self.directory.value().composition.is_some();
        if context_blocked && let Err(error) = self.recipe() {
            column = column.push_child(review_message(error, "project-error", true));
        }
        if let Some(project) = &self.applied {
            column = column
                .push_child(review_label("Approved relaunch context").build(cx))
                .push_child(review_line(
                    format!(
                        "{} · {} {} → {}",
                        project.environment.program(),
                        project.environment.kind(),
                        display_value(project.environment.target()),
                        display_value(&project.directory)
                    ),
                    "project-recipe",
                ));
            if let Some(agent) = &project.agent {
                let suspicious = suspected_credentials(&agent.args);
                sections = sections.push_child(column.build(cx));
                column = review_block(cx, "Agent", "agent-divider");
                if suspicious {
                    column = column.push_child(review_message(
                        "Agent arguments are hidden: suspected credentials. Omit this invocation to save; credentials cannot be persisted.",
                        "agent-secret-warning", true,
                    ));
                } else {
                    let arguments = agent
                        .args
                        .iter()
                        .map(|a| format!("{a:?}"))
                        .collect::<Vec<_>>()
                        .join(" ");
                    column = column.push_child(review_line(
                        format!("Last agent: {} {arguments}", agent.program),
                        "project-agent",
                    ));
                }
                let change = ui.clone();
                column = column.push_child(View::deferred(
                    Checkbox::new("Include last recognized agent", self.include_agent)
                        .style(quick_save_button_style(cx, false))
                        .unwrap()
                        .disabled(pending)
                        .on_change(move |checked| {
                            change.update_quick_save(generation, |s| {
                                s.project.include_agent = checked;
                                s.project.checked_arguments = false;
                            })
                        })
                        .keyed("project-agent-include"),
                ));
                column = column.push_child(review_message(
                    if self.include_agent {
                        "The displayed invocation will be included only after the required argument review. Secret detection is limited."
                    } else {
                        "Agent explicitly omitted — this is not an exact invocation copy. Only the project shell will open."
                    }, "agent-inclusion", false,
                ));
                if self.include_agent && !suspicious {
                    let change = ui.clone();
                    column = column.push_child(View::deferred(
                        Checkbox::new(
                            "I checked agent arguments: no passwords/tokens",
                            self.checked_arguments,
                        )
                        .style(quick_save_button_style(cx, false))
                        .unwrap()
                        .disabled(pending)
                        .on_change(move |checked| {
                            change.update_quick_save(generation, |s| {
                                s.project.checked_arguments = checked
                            })
                        })
                        .keyed("project-agent-ack"),
                    ));
                }
            } else {
                column = column.push_child(review_message(
                    "No recognized agent in this environment; only the project shell will open.",
                    "project-no-agent",
                    false,
                ));
            }
        }
        if !context_blocked && let Err(error) = self.recipe() {
            column = column.push_child(review_message(error, "project-error", true));
        }
        sections.push_child(column.build(cx)).build(cx)
    }
}

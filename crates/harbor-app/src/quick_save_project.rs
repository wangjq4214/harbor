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
        let mut column = Column::new()
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .gap(6.0)
            .unwrap();
        column = column.push_child(review_line(if self.applied.is_none() {
            "Environment / current directory unknown. Enable guided integration or correct below. Initial cwd is not used."
        } else { "Captured project context (untrusted shell metadata; review before saving)." }.into(), "project-status"));
        let mut environments = Row::new().gap(6.0).unwrap();
        for (kind, label) in [
            ("pwsh", "Local pwsh"),
            ("wsl", "WSL bash"),
            ("ssh", "Direct SSH bash"),
        ] {
            let change = ui.clone();
            environments = environments.push_child(View::deferred(
                Button::text(format!(
                    "{} {label}",
                    if self.kind == kind { "[x]" } else { "[ ]" }
                ))
                .disabled(pending)
                .on_click(move |_| {
                    change.update_quick_save(generation, |s| {
                        s.project.kind = kind.into();
                        s.project.target.set_text("");
                    })
                })
                .keyed(format!("project-kind-{kind}")),
            ));
        }
        column = column.push_child(environments.build(cx));
        let target_change = ui.clone();
        let directory_change = ui.clone();
        column = column
            .push_child(
                TextField::new(self.target.clone())
                    .disabled(pending)
                    .placeholder("WSL distribution / SSH destination; empty for local")
                    .on_change(move |_| target_change.update_quick_save(generation, |_| {}))
                    .keyed("project-target")
                    .build(cx),
            )
            .push_child(
                TextField::new(self.directory.clone())
                    .disabled(pending)
                    .placeholder("Absolute current project directory")
                    .on_change(move |_| directory_change.update_quick_save(generation, |_| {}))
                    .keyed("project-directory")
                    .build(cx),
            );
        let change = ui.clone();
        column = column.push_child(View::deferred(
            Button::text("Apply context correction")
                .disabled(pending)
                .on_click(move |_| {
                    change.update_quick_save(generation, |s| {
                        s.error = s.project.apply().err().map(str::to_owned);
                    })
                })
                .keyed("project-apply"),
        ));
        if let Some(project) = &self.applied {
            column = column.push_child(review_line(
                format!(
                    "Relaunch: {} → {} {} → {}",
                    project.environment.program(),
                    project.environment.kind(),
                    project.environment.target(),
                    display_value(&project.directory)
                ),
                "project-recipe",
            ));
            if let Some(agent) = &project.agent {
                let suspicious = suspected_credentials(&agent.args);
                let description = if suspicious {
                    "[arguments hidden: suspected credentials; omit this invocation to save]".into()
                } else {
                    agent
                        .args
                        .iter()
                        .map(|a| format!("{a:?}"))
                        .collect::<Vec<_>>()
                        .join(" ")
                };
                column = column.push_child(review_line(
                    format!("Last agent: {} {description}", agent.program),
                    "project-agent",
                ));
                let change = ui.clone();
                column = column.push_child(View::deferred(
                    Button::text(if self.include_agent {
                        "[x] Include last agent (click to omit)"
                    } else {
                        "[ ] Agent explicitly omitted — not an exact invocation copy"
                    })
                    .disabled(pending)
                    .on_click(move |_| {
                        change.update_quick_save(generation, |s| {
                            s.project.include_agent = !s.project.include_agent;
                            s.project.checked_arguments = false;
                        })
                    })
                    .keyed("project-agent-include"),
                ));
                if self.include_agent && !suspicious {
                    let change = ui.clone();
                    column = column.push_child(View::deferred(
                        Button::text(if self.checked_arguments {
                            "[x] Agent arguments contain no passwords/tokens"
                        } else {
                            "[ ] I checked agent arguments: no passwords/tokens"
                        })
                        .disabled(pending)
                        .on_click(move |_| {
                            change.update_quick_save(generation, |s| {
                                s.project.checked_arguments = !s.project.checked_arguments
                            })
                        })
                        .keyed("project-agent-ack"),
                    ));
                }
            } else {
                column = column.push_child(review_line(
                    "No recognized agent in this environment; only the project shell will open."
                        .into(),
                    "project-no-agent",
                ));
            }
        }
        if self.changed {
            column = column.push_child(review_line("Context explicitly corrected; original recognition is not claimed. Changing environments clears its agent selection.".into(), "project-corrected"));
        }
        column = column.push_child(review_line("Project relaunch uses the environment launcher and fixed directory/agent setup, not the source shell's startup arguments. Secret detection is limited; no history, nesting or virtual-environment restoration.".into(), "project-limitations"));
        if let Err(error) = self.recipe() {
            column = column.push_child(review_line(error.into(), "project-error"));
        }
        column.build(cx)
    }
}

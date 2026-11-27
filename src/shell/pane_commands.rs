//! Production split command ownership. Poll and publish only on the Host turn.
use super::*;
use harbor_app::command::PaneCommand;
use harbor_widget::SplitAxis;

pub(super) struct PendingSplit {
    pub(super) operation: crate::split_launch::SplitOperation,
    axis: SplitAxis,
    size: harbor_terminal::TerminalSize,
    palette: bool,
    auth_draw: Option<harbor_widget::scene::primitive::ExternalDrawId>,
}

impl ActiveSession {
    pub(super) fn begin_pane_split(
        &mut self,
        command: PaneCommand,
        palette: bool,
    ) -> Result<(), &'static str> {
        if self.pending_split.is_some() || self.config.has_pending() {
            return Err("Rejected: session configuration or startup is pending");
        }
        let size = self
            .tabs
            .split_source_size()
            .ok_or("Rejected: no live split source")?;
        let operation = self
            .tabs
            .begin_split()
            .map_err(|_| "Failed: split could not start")?;
        self.pending_split = Some(PendingSplit {
            operation,
            size,
            palette,
            auth_draw: None,
            axis: if command == PaneCommand::SplitHorizontal {
                SplitAxis::Horizontal
            } else {
                SplitAxis::Vertical
            },
        });
        Ok(())
    }

    pub(super) fn advance_pane_split(&mut self) -> Option<ControlFlowEffect> {
        use crate::split_launch::SplitPoll;
        let mut pending = self.pending_split.take()?;
        let (target, _) = pending.operation.target();
        let mut wait = None;
        let poll = self.tabs.poll_split(
            &mut pending.operation,
            pending.size,
            self.main_host.viewport().physical_size,
            Instant::now(),
        );
        if let SplitPoll::Pending = poll {
            let resources = pending.operation.pending_resources();
            let draw = resources.map(|resources| resources.bridge().draw_id());
            if draw != pending.auth_draw {
                merge_wait(
                    &mut wait,
                    self.main_host.cancel_active_input_ownership().wait,
                );
                self.workspace_ui.close_split_authentication(target);
                if let Some(resources) = resources {
                    // Draw remains projection-only; the Host admits one reserved
                    // authentication resize per turn below.
                    if let Ok(mut terminal) = resources.terminal().lock() {
                        terminal.use_host_resize_scheduling();
                    }
                    let focus = self
                        .workspace_ui
                        .show_split_authentication(target, resources.bridge().clone());
                    merge_wait(&mut wait, self.main_host.request_focus(&focus).wait);
                } else {
                    merge_wait(
                        &mut wait,
                        self.main_host
                            .request_focus(&self.workspace_ui.terminal_focus())
                            .wait,
                    );
                }
                merge_wait(&mut wait, self.main_host.request_frame().wait);
                pending.auth_draw = draw;
            }
            let viewport = self.main_host.viewport();
            if self.tabs.advance_split_authentication(
                target,
                viewport.scale_factor,
                viewport.physical_size,
            ) {
                merge_wait(&mut wait, self.main_host.request_frame().wait);
            }
            self.pending_split = Some(pending);
            return wait;
        }
        merge_wait(
            &mut wait,
            self.main_host.cancel_active_input_ownership().wait,
        );
        self.workspace_ui.close_split_authentication(target);
        let completion = match poll {
            SplitPoll::Ready(ready) => {
                match self
                    .tabs
                    .attach_split(*ready, pending.operation.id(), pending.axis)
                {
                    Ok((outcome, _kind)) => {
                        // This never activates another workspace. If the source workspace
                        // is still active, focus the actual reserved result, including maximize.
                        self.tabs.focus_split_result(target);
                        let result = self.tabs.apply_tab_outcome(
                            &mut self.main_host,
                            outcome,
                            harbor_app::tab_view::TabFocusPolicy::Terminal,
                        );
                        merge_wait(&mut wait, result.wait);
                        Ok(())
                    }
                    Err(_) => Err("Rejected: split source or result retired"),
                }
            }
            SplitPoll::Failed(error) => {
                // Launch errors can contain source arguments/paths. Preserve the
                // honest failure without disclosing arbitrary launch data.
                drop(error);
                Err("Failed: split launch failed; surviving sessions are unchanged")
            }
            SplitPoll::Cancelled | SplitPoll::Finished => {
                Err("Cancelled: split source or operation retired")
            }
            SplitPoll::Pending => unreachable!(),
        };
        self.tabs.cancel_split_reservation(target);
        if pending.palette {
            self.workspace_ui.report_palette_result(
                AppCommand::Pane(if pending.axis == SplitAxis::Horizontal {
                    PaneCommand::SplitHorizontal
                } else {
                    PaneCommand::SplitVertical
                }),
                completion,
            );
        } else if let Err(reason) = completion {
            self.workspace_ui.report_configuration_result(reason.into());
        }
        merge_wait(
            &mut wait,
            self.main_host
                .request_focus(&self.workspace_ui.terminal_focus())
                .wait,
        );
        merge_wait(&mut wait, self.main_host.request_frame().wait);
        merge_wait(&mut wait, self.reconcile_clipboard());
        if self.config.take_reload_after_split() {
            let (_, reload_wait) = self.config.reload(
                &mut self.tabs,
                &self.workspace_ui,
                &mut self.main_host,
                true,
                false,
            );
            merge_wait(&mut wait, reload_wait);
        }
        wait
    }
}

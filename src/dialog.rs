//! In-window paste confirmation controller, input gating, and target binding validation.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::effects::merge_wait;
use harbor_app::command::CommandCompletion;
use harbor_app::tab_manager::PaneBinding;
use harbor_terminal::{InputModes, PasteDisposition, Terminal};
use harbor_widget::effects::ControlFlowEffect;
use harbor_widget::winit::{FrameError, WinitWindowHost};

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) enum DialogOutcome {
    None,
    Cancelled,
    Confirmed(String),
    Fatal(FrameError),
}

/// Encodes the unchanged confirmation text with the modes current at the
/// moment of confirmation, then performs exactly one Host-owned PTY write.
pub(crate) fn write_confirmation_outcome<E>(
    outcome: &DialogOutcome,
    input_modes: InputModes,
    write: impl FnOnce(&[u8]) -> Result<(), E>,
) -> Result<bool, E> {
    let DialogOutcome::Confirmed(raw_text) = outcome else {
        return Ok(false);
    };
    let bytes = input_modes.paste(raw_text.as_bytes());
    write(bytes.as_ref())?;
    Ok(true)
}

/// Outcome of resolving an in-window paste confirmation request.
#[derive(Debug)]
pub(crate) enum PasteResolution {
    Ignored,
    Resolved {
        palette_completion: Option<Result<CommandCompletion, &'static str>>,
    },
    Fatal {
        error: FrameError,
    },
}

struct PendingPaste {
    generation: u64,
    raw_text: String,
    target: PasteTarget,
    palette_invocation: bool,
}

/// Controller encapsulating modal paste confirmation and clipboard interaction.
pub(crate) struct PasteController {
    pending: Option<PendingPaste>,
    input_gate: Arc<AtomicBool>,
}

impl PasteController {
    pub(crate) fn new(input_gate: Arc<AtomicBool>) -> Self {
        Self {
            pending: None,
            input_gate,
        }
    }

    pub(crate) fn is_active(&self) -> bool {
        self.pending.is_some()
    }

    pub(crate) fn generation(&self) -> Option<u64> {
        self.pending.as_ref().map(|pending| pending.generation)
    }

    pub(crate) fn source(&self) -> Option<PaneBinding> {
        self.pending.as_ref().map(|pending| pending.target.binding)
    }

    pub(crate) fn retire_target(
        &mut self,
        workspace_ui: &harbor_app::workspace::WorkspaceUiController,
        other_confirmation_active: bool,
    ) -> Option<Result<CommandCompletion, &'static str>> {
        let report = self
            .pending
            .as_ref()
            .is_some_and(|pending| pending.palette_invocation);
        self.cancel(workspace_ui, other_confirmation_active);
        report.then_some(Err("Rejected: paste target retired"))
    }

    pub(crate) fn cancel(
        &mut self,
        workspace_ui: &harbor_app::workspace::WorkspaceUiController,
        other_confirmation_active: bool,
    ) -> bool {
        let closed = if let Some(pending) = self.pending.take() {
            workspace_ui.close_confirmation(pending.generation);
            true
        } else {
            false
        };
        self.input_gate
            .store(other_confirmation_active, Ordering::Release);
        closed
    }

    pub(crate) fn cancel_for_shutdown(&mut self) {
        self.pending = None;
        self.input_gate.store(false, Ordering::Release);
    }

    pub(crate) fn paste_text(
        &mut self,
        workspace_ui: &harbor_app::workspace::WorkspaceUiController,
        active_terminal: Option<&Arc<Mutex<Terminal>>>,
        active_binding: Option<PaneBinding>,
        raw_text: String,
        palette_invocation: bool,
        other_confirmation_active: bool,
    ) -> Result<(Option<harbor_widget::FocusHandle>, CommandCompletion), &'static str> {
        if self.is_active() || other_confirmation_active {
            return Err("Rejected: paste confirmation is active");
        }
        let binding = active_binding.ok_or("Rejected: no live pane for paste")?;
        let terminal = active_terminal.ok_or("Rejected: no active terminal for paste")?;
        let target = PasteTarget::new(binding, terminal);
        match prepare_paste(active_terminal, &raw_text)? {
            PasteDisposition::SendDirect => Ok((
                None,
                if raw_text.is_empty() {
                    CommandCompletion::Unchanged
                } else {
                    CommandCompletion::Applied
                },
            )),
            PasteDisposition::Confirm { raw_text } => {
                let line_count = raw_text.lines().count();
                let (generation, focus) = workspace_ui.open_confirmation(
                    harbor_app::confirmation::ConfirmationRequest::paste(
                        line_count,
                        raw_text.clone(),
                    ),
                );
                self.pending = Some(PendingPaste {
                    generation,
                    raw_text,
                    target,
                    palette_invocation,
                });
                self.input_gate.store(true, Ordering::Release);
                Ok((Some(focus), CommandCompletion::PendingConfirmation))
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn resolve_confirmation(
        &mut self,
        generation: u64,
        confirmed: bool,
        workspace_ui: &harbor_app::workspace::WorkspaceUiController,
        active_terminal: Option<&Arc<Mutex<Terminal>>>,
        active_binding: Option<PaneBinding>,
        gpu_fault: Option<harbor_widget::winit::GpuFault>,
        other_confirmation_active: bool,
    ) -> PasteResolution {
        if self
            .pending
            .as_ref()
            .is_none_or(|pending| pending.generation != generation)
        {
            return PasteResolution::Ignored;
        }
        if let Some(fault) = gpu_fault {
            self.cancel(workspace_ui, other_confirmation_active);
            return PasteResolution::Fatal {
                error: FrameError::SharedGpu(fault),
            };
        }
        let pending = self.pending.take().expect("matched pending paste");
        workspace_ui.close_confirmation(generation);
        self.input_gate
            .store(other_confirmation_active, Ordering::Release);
        let outcome = if confirmed {
            DialogOutcome::Confirmed(pending.raw_text)
        } else {
            DialogOutcome::Cancelled
        };
        let completion = complete_paste_confirmation(
            &outcome,
            Some(&pending.target),
            active_binding.zip(active_terminal),
        );
        PasteResolution::Resolved {
            palette_completion: pending.palette_invocation.then_some(completion),
        }
    }

    pub(crate) fn paste_from_clipboard(
        &mut self,
        workspace_ui: &harbor_app::workspace::WorkspaceUiController,
        main_host: &mut WinitWindowHost,
        active_terminal: Option<&Arc<Mutex<Terminal>>>,
        active_binding: Option<PaneBinding>,
        palette_invocation: bool,
        other_confirmation_active: bool,
    ) -> Result<(Option<ControlFlowEffect>, CommandCompletion), &'static str> {
        if self.is_active() || other_confirmation_active {
            return Err("Rejected: paste confirmation is active");
        }
        if active_binding.is_none() {
            return Err("Rejected: no live pane for paste");
        }
        if active_terminal.is_none() {
            return Err("Rejected: no active terminal for paste");
        }
        let raw_text = arboard::Clipboard::new()
            .and_then(|mut clipboard| clipboard.get_text())
            .map_err(|error| {
                tracing::warn!(error = %error, "failed to read clipboard text");
                "Failed: clipboard text unavailable"
            })?;
        let (focus, completion) = self.paste_text(
            workspace_ui,
            active_terminal,
            active_binding,
            raw_text,
            palette_invocation,
            other_confirmation_active,
        )?;
        let wait = if let Some(focus) = focus {
            let mut wait = main_host.cancel_active_input_ownership().wait;
            merge_wait(&mut wait, main_host.request_focus(&focus).wait);
            merge_wait(&mut wait, main_host.request_frame().wait);
            wait
        } else {
            None
        };
        Ok((wait, completion))
    }
}

/// The production decision/write seam: confirmation never writes bytes before approval.
fn prepare_paste(
    active_terminal: Option<&Arc<Mutex<Terminal>>>,
    raw_text: &str,
) -> Result<PasteDisposition, &'static str> {
    let terminal = active_terminal.ok_or("Rejected: no active terminal for paste")?;
    let mut terminal = terminal
        .lock()
        .map_err(|_| "Failed: terminal lock unavailable for paste")?;
    let input_modes = terminal.drain_and_snapshot().input_modes;
    if terminal.is_session_closed() || terminal.shell_process_exited() {
        return Err("Rejected: paste target retired");
    }
    let disposition = PasteDisposition::decide(input_modes, raw_text);
    if matches!(disposition, PasteDisposition::SendDirect) {
        terminal.write_pty(input_modes.paste(raw_text.as_bytes()).as_ref()).map_err(|error| {
            tracing::warn!(error = %format_args!("{error:#}"), "failed to write clipboard paste");
            "Failed: could not write clipboard paste"
        })?;
    }
    Ok(disposition)
}

/// Retain identity, not ownership: a dialog cannot keep retired I/O alive.
struct PasteTarget {
    binding: PaneBinding,
    terminal: std::sync::Weak<Mutex<Terminal>>,
}
impl PasteTarget {
    fn new(binding: PaneBinding, terminal: &Arc<Mutex<Terminal>>) -> Self {
        Self {
            binding,
            terminal: Arc::downgrade(terminal),
        }
    }
}

/// Revalidate the original live target; use its modes at approval, never at capture.
fn complete_paste_confirmation(
    outcome: &DialogOutcome,
    target: Option<&PasteTarget>,
    current: Option<(PaneBinding, &Arc<Mutex<Terminal>>)>,
) -> Result<CommandCompletion, &'static str> {
    let DialogOutcome::Confirmed(raw_text) = outcome else {
        return Err("Cancelled: paste was not confirmed");
    };
    let target = target.ok_or("Rejected: paste target retired")?;
    let (binding, terminal) = current.ok_or("Rejected: paste target retired")?;
    if target.binding != binding || !target.terminal.ptr_eq(&Arc::downgrade(terminal)) {
        return Err("Rejected: paste target changed");
    }
    let mut terminal = terminal
        .lock()
        .map_err(|_| "Failed: terminal lock unavailable for confirmed paste")?;
    let modes = terminal.drain_and_snapshot().input_modes;
    if terminal.is_session_closed() || terminal.shell_process_exited() {
        return Err("Rejected: paste target retired");
    }
    write_confirmation_outcome(outcome, modes, |bytes| terminal.write_pty(bytes)).map_err(
        |error| {
            tracing::warn!(error = %format_args!("{error:#}"), "failed to write confirmed paste");
            "Failed: could not write confirmed paste"
        },
    )?;
    Ok(if raw_text.is_empty() {
        CommandCompletion::Unchanged
    } else {
        CommandCompletion::Applied
    })
}

#[cfg(test)]
mod tests;

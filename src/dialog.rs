//! In-window confirmation coordinator (paste and OSC 52), input gating, and target validation.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::effects::merge_wait;
use harbor_app::command::CommandCompletion;
use harbor_app::tab_manager::{PaneBinding, SessionClipboardWrite, SessionId};
use harbor_terminal::{InputModes, PasteDisposition, Terminal};
use harbor_widget::FocusHandle;
use harbor_widget::effects::ControlFlowEffect;
use harbor_widget::winit::WinitWindowHost;

/// Encodes the unchanged confirmation text with the modes current at the
/// moment of confirmation, then performs exactly one Host-owned PTY write.
pub(crate) fn write_confirmation_outcome<E>(
    confirmed_text: Option<&str>,
    input_modes: InputModes,
    write: impl FnOnce(&[u8]) -> Result<(), E>,
) -> Result<bool, E> {
    let Some(raw_text) = confirmed_text else {
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
}

struct PendingPaste {
    generation: u64,
    raw_text: String,
    target: PasteTarget,
    palette_invocation: bool,
}

#[derive(Default)]
struct PasteController {
    pending: Option<PendingPaste>,
}

/// Single-writer coordinator for mutually exclusive in-window paste and OSC 52
/// confirmations, owning the shared PTY input gate.
pub(crate) struct ConfirmationCoordinator {
    paste: PasteController,
    clipboard: crate::clipboard::ClipboardPolicy,
    clipboard_epoch: u64,
    input_gate: Arc<AtomicBool>,
}

impl ConfirmationCoordinator {
    pub(crate) fn new(
        input_gate: Arc<AtomicBool>,
        clipboard_policy: harbor_config::Osc52WritePolicy,
    ) -> Self {
        Self {
            paste: PasteController::default(),
            clipboard: crate::clipboard::ClipboardPolicy::new(clipboard_policy),
            clipboard_epoch: 0,
            input_gate,
        }
    }

    pub(crate) fn is_active(&self) -> bool {
        self.paste.pending.is_some() || self.clipboard.is_active()
    }

    fn sync_input_gate(&self) {
        self.input_gate.store(self.is_active(), Ordering::Release);
    }

    pub(crate) fn paste_generation(&self) -> Option<u64> {
        self.paste
            .pending
            .as_ref()
            .map(|pending| pending.generation)
    }

    pub(crate) fn paste_source(&self) -> Option<PaneBinding> {
        self.paste
            .pending
            .as_ref()
            .map(|pending| pending.target.binding)
    }

    pub(crate) fn clipboard_generation(&self) -> Option<u64> {
        self.clipboard.generation()
    }

    pub(crate) fn clipboard_source(&self) -> Option<SessionId> {
        self.clipboard.source()
    }

    pub(crate) fn clipboard_epoch(&self) -> u64 {
        self.clipboard_epoch
    }

    #[cfg(test)]
    pub(crate) fn retain_clipboard_for_test(
        &mut self,
        generation: u64,
        request: SessionClipboardWrite,
    ) {
        self.clipboard.retain(generation, request);
        self.sync_input_gate();
    }

    pub(crate) fn clipboard_admission(&self, eligible: bool) -> crate::clipboard::Admission {
        self.clipboard.admission(eligible && !self.is_active())
    }

    pub(crate) fn open_clipboard_confirmation(
        &mut self,
        workspace_ui: &harbor_app::workspace::WorkspaceUiController,
        foreground_available: bool,
        current_epoch: u64,
        request: SessionClipboardWrite,
    ) -> Option<FocusHandle> {
        if self.is_active() {
            return None;
        }
        if !foreground_available {
            tracing::warn!("OSC 52 confirmation denied: foreground monitor unavailable");
            return None;
        }
        self.clipboard_epoch = current_epoch;
        let (header, shown) = crate::clipboard::confirmation_presentation(&request);
        let (generation, focus) = workspace_ui.open_confirmation(
            harbor_app::confirmation::ConfirmationRequest::clipboard_write(header, shown),
        );
        self.clipboard.retain(generation, request);
        self.sync_input_gate();
        Some(focus)
    }

    pub(crate) fn cancel_clipboard(
        &mut self,
        workspace_ui: &harbor_app::workspace::WorkspaceUiController,
    ) -> bool {
        let Some(generation) = self.clipboard.generation() else {
            return false;
        };
        self.clipboard.cancel();
        workspace_ui.close_confirmation(generation);
        self.sync_input_gate();
        tracing::debug!("OSC 52 confirmation cancelled");
        true
    }

    pub(crate) fn resolve_clipboard(
        &mut self,
        generation: u64,
        approved: bool,
        eligible: bool,
        workspace_ui: &harbor_app::workspace::WorkspaceUiController,
    ) -> Option<SessionClipboardWrite> {
        if self.clipboard.generation() != Some(generation) {
            return None;
        }
        workspace_ui.close_confirmation(generation);
        let request = self.clipboard.resolve(approved, eligible);
        self.sync_input_gate();
        request
    }

    pub(crate) fn retire_paste_target(
        &mut self,
        workspace_ui: &harbor_app::workspace::WorkspaceUiController,
    ) -> Option<Result<CommandCompletion, &'static str>> {
        let report = self
            .paste
            .pending
            .as_ref()
            .is_some_and(|pending| pending.palette_invocation);
        self.cancel_paste(workspace_ui);
        report.then_some(Err("Rejected: paste target retired"))
    }

    pub(crate) fn cancel_paste(
        &mut self,
        workspace_ui: &harbor_app::workspace::WorkspaceUiController,
    ) -> bool {
        let closed = if let Some(pending) = self.paste.pending.take() {
            workspace_ui.close_confirmation(pending.generation);
            true
        } else {
            false
        };
        self.sync_input_gate();
        closed
    }

    pub(crate) fn cancel_for_shutdown(&mut self) {
        self.paste.pending = None;
        self.clipboard.cancel();
        self.input_gate.store(false, Ordering::Release);
    }

    pub(crate) fn paste_text(
        &mut self,
        workspace_ui: &harbor_app::workspace::WorkspaceUiController,
        active_terminal: Option<&Arc<Mutex<Terminal>>>,
        active_binding: Option<PaneBinding>,
        raw_text: String,
        palette_invocation: bool,
    ) -> Result<(Option<FocusHandle>, CommandCompletion), &'static str> {
        if self.is_active() {
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
                self.paste.pending = Some(PendingPaste {
                    generation,
                    raw_text,
                    target,
                    palette_invocation,
                });
                self.sync_input_gate();
                Ok((Some(focus), CommandCompletion::PendingConfirmation))
            }
        }
    }

    pub(crate) fn resolve_paste(
        &mut self,
        generation: u64,
        confirmed: bool,
        workspace_ui: &harbor_app::workspace::WorkspaceUiController,
        active_terminal: Option<&Arc<Mutex<Terminal>>>,
        active_binding: Option<PaneBinding>,
    ) -> PasteResolution {
        if self
            .paste
            .pending
            .as_ref()
            .is_none_or(|pending| pending.generation != generation)
        {
            return PasteResolution::Ignored;
        }
        let pending = self.paste.pending.take().expect("matched pending paste");
        workspace_ui.close_confirmation(generation);
        self.sync_input_gate();
        let confirmed_text = confirmed.then_some(pending.raw_text.as_str());
        let completion = complete_paste_confirmation(
            confirmed_text,
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
    ) -> Result<(Option<ControlFlowEffect>, CommandCompletion), &'static str> {
        if self.is_active() {
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
    confirmed_text: Option<&str>,
    target: Option<&PasteTarget>,
    current: Option<(PaneBinding, &Arc<Mutex<Terminal>>)>,
) -> Result<CommandCompletion, &'static str> {
    let Some(raw_text) = confirmed_text else {
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
    write_confirmation_outcome(Some(raw_text), modes, |bytes| terminal.write_pty(bytes)).map_err(
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

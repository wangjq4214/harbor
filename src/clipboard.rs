//! Host-owned OSC 52 authorization. No terminal request is an authenticity claim.
pub(crate) mod foreground_monitor;

use harbor_app::tab_manager::{SessionClipboardWrite, TabId};
use harbor_config::Osc52WritePolicy;
use harbor_terminal::ClipboardDelivery;
use winit::window::Window;

pub(crate) const PREVIEW_CHARS: usize = 1024;

pub(crate) fn delivery(policy: Osc52WritePolicy) -> ClipboardDelivery {
    match policy {
        Osc52WritePolicy::Allow => ClipboardDelivery::Latest,
        Osc52WritePolicy::Deny => ClipboardDelivery::Discard,
        Osc52WritePolicy::Confirm => ClipboardDelivery::First,
    }
}

/// Snapshot at admission/execution, including only this request's confirmation window.
pub(crate) struct Eligibility {
    pub(crate) active_live: bool,
    pub(crate) main_foreground: bool,
    pub(crate) confirmation_foreground: bool,
    pub(crate) minimized: bool,
}
impl Eligibility {
    pub(crate) fn permits(&self, continuation: bool) -> bool {
        self.active_live
            && !self.minimized
            && (self.main_foreground || (continuation && self.confirmation_foreground))
    }
}

/// Query actual foreground ownership on Windows, rather than trusting delayed focus events.
pub(crate) fn foreground(window: &Window) -> bool {
    if window.is_minimized() == Some(true) || window.is_visible() == Some(false) {
        return false;
    }
    #[cfg(windows)]
    {
        use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
        let Ok(handle) = window.window_handle() else {
            return false;
        };
        let RawWindowHandle::Win32(handle) = handle.as_raw() else {
            return false;
        };
        // Read-only native foreground observation; no activation or clipboard access.
        unsafe {
            windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow().0 as isize
                == handle.hwnd.get()
        }
    }
    #[cfg(not(windows))]
    {
        window.has_focus()
    }
}

pub(crate) fn preview(text: &str) -> String {
    let mut chars = text.chars();
    let mut preview: String = chars.by_ref().take(PREVIEW_CHARS).collect();
    if chars.next().is_some() {
        preview.push_str("\n[preview truncated]");
    }
    if preview.is_empty() {
        preview.push_str("[clear clipboard]");
    }
    preview
}
pub(crate) fn confirmation_presentation(request: &SessionClipboardWrite) -> (String, String) {
    (
        format!(
            "Tab {} requests clipboard write ({} bytes)",
            request.source,
            request.contents.as_str().len()
        ),
        preview(request.contents.as_str()),
    )
}

/// The native effect is called exactly once only after source/focus revalidation.
/// Return its real error; never convert failure into a successful permission decision.
pub(crate) fn execute<E>(
    request: SessionClipboardWrite,
    eligible: bool,
    sink: impl FnOnce(String) -> Result<(), E>,
) -> Result<bool, E> {
    if !eligible {
        return Ok(false);
    }
    sink(request.contents.into_text())?;
    Ok(true)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Admission {
    Denied,
    Write,
    Confirm,
}

/// Payload retained only for the single immutable admitted confirmation.
pub(crate) struct ClipboardPolicy {
    policy: Osc52WritePolicy,
    pending: Option<SessionClipboardWrite>,
}

impl ClipboardPolicy {
    pub(crate) fn new(policy: Osc52WritePolicy) -> Self {
        Self {
            policy,
            pending: None,
        }
    }
    #[cfg(test)]
    pub(crate) fn pending(&self) -> Option<&SessionClipboardWrite> {
        self.pending.as_ref()
    }
    pub(crate) fn admission(&self, eligible: bool) -> Admission {
        if !eligible || self.pending.is_some() {
            return Admission::Denied;
        }
        match self.policy {
            Osc52WritePolicy::Allow => Admission::Write,
            Osc52WritePolicy::Deny => Admission::Denied,
            Osc52WritePolicy::Confirm => Admission::Confirm,
        }
    }
    pub(crate) fn retain(&mut self, request: SessionClipboardWrite) {
        debug_assert!(self.pending.is_none());
        self.pending = Some(request);
    }
    pub(crate) fn cancel(&mut self) {
        self.pending = None;
    }
    pub(crate) fn resolve(
        &mut self,
        approved: bool,
        eligible: bool,
    ) -> Option<SessionClipboardWrite> {
        let request = self.pending.take();
        if approved && eligible { request } else { None }
    }
    pub(crate) fn source(&self) -> Option<TabId> {
        self.pending.as_ref().map(|r| r.source)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use harbor_terminal::{Terminal, TerminalOutputEvent};

    fn request(id: u64) -> SessionClipboardWrite {
        let mut terminal = Terminal::new_headless(2, 8);
        terminal.process_output(b"\x1b]52;c;Zmlyc3Q=\x07");
        let event = terminal.drain_output_events().pop().unwrap();
        let TerminalOutputEvent::ClipboardWrite(contents) = event else {
            panic!("fixture request")
        };
        SessionClipboardWrite {
            source: TabId(id),
            contents,
        }
    }

    #[test]
    fn every_policy_rejects_ineligible_and_pending_candidates() {
        for mode in [
            Osc52WritePolicy::Allow,
            Osc52WritePolicy::Deny,
            Osc52WritePolicy::Confirm,
        ] {
            let mut policy = ClipboardPolicy::new(mode);
            assert_eq!(policy.admission(false), Admission::Denied);
            let expected = match mode {
                Osc52WritePolicy::Allow => Admission::Write,
                Osc52WritePolicy::Deny => Admission::Denied,
                Osc52WritePolicy::Confirm => Admission::Confirm,
            };
            assert_eq!(policy.admission(true), expected);
            policy.retain(request(7));
            for _ in 0..100 {
                assert_eq!(policy.admission(true), Admission::Denied);
            }
            assert_eq!(policy.source(), Some(TabId(7)));
            assert_eq!(policy.pending().unwrap().contents.as_str(), "first");
        }
    }

    #[test]
    fn confirmation_is_single_use_source_preserving_and_not_a_grant() {
        let mut policy = ClipboardPolicy::new(Osc52WritePolicy::Confirm);
        policy.retain(request(9));
        let approved = policy.resolve(true, true).unwrap();
        assert_eq!(approved.source, TabId(9));
        assert_eq!(approved.contents.as_str(), "first");
        assert!(policy.resolve(true, true).is_none());
        assert_eq!(policy.admission(true), Admission::Confirm);
        for (approval, eligibility) in [(false, true), (true, false), (false, false)] {
            policy.retain(request(9));
            assert!(policy.resolve(approval, eligibility).is_none());
            assert!(policy.resolve(true, true).is_none());
        }
        policy.retain(request(9));
        policy.cancel();
        assert!(policy.resolve(true, true).is_none());
    }
    #[test]
    fn confirmation_presentation_shows_stable_source_and_decoded_bytes_not_encoded_size() {
        let request = SessionClipboardWrite {
            source: TabId(41),
            contents: harbor_terminal::ClipboardWrite::new("界\n\t".into()).unwrap(),
        };
        let (header, shown) = confirmation_presentation(&request);
        assert!(header.contains("Tab 41"));
        assert!(header.contains("(5 bytes)"));
        assert_eq!(shown, "界\n\t");
        let clear = SessionClipboardWrite {
            source: TabId(41),
            contents: harbor_terminal::ClipboardWrite::new(String::new()).unwrap(),
        };
        assert!(confirmation_presentation(&clear).0.contains("(0 bytes)"));
        let mut calls = 0;
        assert_eq!(
            execute(clear, true, |text| {
                calls += 1;
                assert!(text.is_empty());
                Ok::<_, ()>(())
            }),
            Ok(true)
        );
        assert_eq!(calls, 1);
    }

    #[test]
    fn permission_matrix_and_confirmation_focus_exception_are_narrow() {
        for active_live in [false, true] {
            for minimized in [false, true] {
                for main_foreground in [false, true] {
                    for confirmation_foreground in [false, true] {
                        let eligibility = Eligibility {
                            active_live,
                            minimized,
                            main_foreground,
                            confirmation_foreground,
                        };
                        let fresh = active_live && !minimized && main_foreground;
                        let continuing = active_live
                            && !minimized
                            && (main_foreground || confirmation_foreground);
                        assert_eq!(eligibility.permits(false), fresh);
                        assert_eq!(eligibility.permits(true), continuing);
                        for mode in [
                            Osc52WritePolicy::Allow,
                            Osc52WritePolicy::Deny,
                            Osc52WritePolicy::Confirm,
                        ] {
                            let policy = ClipboardPolicy::new(mode);
                            assert_eq!(
                                policy.admission(eligibility.permits(false)) == Admission::Denied,
                                !fresh || mode == Osc52WritePolicy::Deny
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn recording_and_failing_effects_preserve_authorization_and_actual_outcome() {
        let mut recorded = Vec::new();
        assert_eq!(
            execute(request(1), true, |text| {
                recorded.push(text);
                Ok::<_, ()>(())
            }),
            Ok(true)
        );
        assert_eq!(recorded, ["first"]);
        assert_eq!(
            execute(request(1), false, |_| -> Result<(), ()> {
                panic!("ineligible effect")
            }),
            Ok(false)
        );
        assert_eq!(
            execute(request(1), true, |_| Err::<(), _>("fixture failure")),
            Err("fixture failure")
        );
    }

    #[test]
    fn preview_is_bounded_at_unicode_boundaries_and_clear_is_explicit() {
        let text = "界\t\n".repeat(1_400_000);
        let shown = preview(&text);
        assert!(shown.len() <= PREVIEW_CHARS * 4 + 32);
        assert!(shown.ends_with("[preview truncated]"));
        assert_eq!(preview(""), "[clear clipboard]");
        assert_eq!(preview("x\n\ty"), "x\n\ty");
    }
}

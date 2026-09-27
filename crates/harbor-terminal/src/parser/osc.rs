//! Static routing of completed, bounded OSC payloads into terminal-internal actions.
use super::{osc_color, osc_title, osc7, osc8, osc133};

pub(super) enum Action {
    Title(osc_title::Action),
    WorkingDirectory(osc7::Action),
    Hyperlink(osc8::Action),
    Color(osc_color::Action, bool),
    ShellIntegration(osc133::Action),
}

/// `None` means consume-ignore, including unknown or malformed commands.
pub(super) fn parse(command: &[u8], payload: &[u8], bell_terminated: bool) -> Option<Action> {
    match command {
        b"0" | b"1" | b"2" => osc_title::parse(payload).map(Action::Title),
        b"7" => osc7::action(payload).map(Action::WorkingDirectory),
        b"8" => osc8::parse(payload).map(Action::Hyperlink),
        b"10" | b"11" | b"12" | b"110" | b"111" | b"112" => {
            osc_color::parse(command, payload).map(|action| Action::Color(action, bell_terminated))
        }
        b"133" => osc133::action(payload).map(Action::ShellIntegration),
        _ => None,
    }
}

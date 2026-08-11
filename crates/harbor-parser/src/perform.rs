//! Sink trait for fully recognized VT actions.

use crate::params::Params;

/// Sink for fully recognized VT actions. Implementors provide sequence side effects.
///
/// Core never mentions `Screen`; handlers implement this trait.
pub trait VtHandler {
    fn print(&mut self, ch: char);

    fn execute(&mut self, byte: u8);

    /// The optional marker preserves the CSI private prefix (`?`, `>`, `<`, or `=`).
    fn csi_dispatch(
        &mut self,
        params: &Params,
        intermediates: &[u8],
        action: u8,
        private_marker: Option<u8>,
    );

    fn esc_dispatch(&mut self, intermediates: &[u8], byte: u8);

    /// `params` are OSC semicolon-separated slices (may be empty).
    fn osc_dispatch(&mut self, params: &[&[u8]], bell_terminated: bool);

    /// DCS introducer complete (final byte). Payload follows via `dcs_put` until `dcs_unhook`.
    fn dcs_hook(&mut self, params: &Params, intermediates: &[u8], action: u8);

    fn dcs_put(&mut self, byte: u8);

    /// Ends a hooked DCS/APC/PM/SOS string.
    ///
    /// `terminated` is `true` for a completed 7-bit or enabled 8-bit ST, and
    /// `false` when CAN/SUB cancels the string.
    fn dcs_unhook(&mut self, terminated: bool);

    /// APC/PM/SOS start; payload via `dcs_put` until `dcs_unhook`.
    /// `kind` is introducer final (`b'_'`, `b'^'`, `b'X'`).
    fn start_string(&mut self, kind: u8);
}

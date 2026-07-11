//! Sink trait for fully recognized VT actions.

use super::params::Params;

/// Sink for fully recognized VT actions. Implementors provide sequence side effects.
///
/// Core never mentions `Screen`; handlers implement this trait.
pub(super) trait Perform {
    fn print(&mut self, ch: char);

    fn execute(&mut self, byte: u8);

    fn csi_dispatch(
        &mut self,
        params: &Params,
        intermediates: &[u8],
        ignore: bool,
        private_marker: Option<u8>,
        action: u8,
    );

    fn esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8);

    /// `params` are OSC semicolon-separated slices (may be empty).
    fn osc_dispatch(&mut self, params: &[&[u8]], bell_terminated: bool);

    /// DCS introducer complete (final byte). Payload follows via `put` until `unhook`.
    fn hook(&mut self, params: &Params, intermediates: &[u8], ignore: bool, action: u8);

    fn put(&mut self, byte: u8);

    fn unhook(&mut self);

    /// APC/PM/SOS start; payload via `put` until `unhook`.
    /// `kind` is introducer final (`b'_'`, `b'^'`, `b'X'`).
    fn start_string(&mut self, kind: u8);
}

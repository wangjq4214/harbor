//! PTY shim — re-exports from `harbor-pty` with a winit `WakeHandler` adapter.

pub(crate) use harbor_pty::Pty;
use harbor_pty::WakeHandler;

use crate::event::AppEvent;
use winit::event_loop::EventLoopProxy;
/// Adapter that bridges `EventLoopProxy<AppEvent>` to `harbor_pty::WakeHandler`.
pub(crate) struct PtyWakeHandler(EventLoopProxy<AppEvent>);

impl PtyWakeHandler {
    pub(crate) fn new(proxy: EventLoopProxy<AppEvent>) -> Self {
        Self(proxy)
    }
}

impl WakeHandler for PtyWakeHandler {
    fn wake(&self) -> bool {
        self.0.send_event(AppEvent::PtyOutputReady).is_ok()
    }
}

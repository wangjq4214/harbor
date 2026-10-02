//! Remember external foreground departures, including away/return before the next host turn.
//! OUTOFCONTEXT events are queued in order on the registering winit UI thread.
#[cfg(windows)]
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(windows)]
use windows::Win32::{
    Foundation::HWND,
    UI::{
        Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent},
        WindowsAndMessaging::{
            EVENT_SYSTEM_FOREGROUND, WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS,
        },
    },
};

#[cfg(windows)]
static DEPARTURES: AtomicU64 = AtomicU64::new(0);

#[cfg(windows)]
unsafe extern "system" fn observe_external_foreground(
    _hook: HWINEVENTHOOK,
    _event: u32,
    _window: HWND,
    _object: i32,
    _child: i32,
    _thread: u32,
    _time: u32,
) {
    // SKIPOWNPROCESS excludes both our main and our independent confirmation windows.
    // No window labels, terminal bytes or clipboard content are observed or retained.
    DEPARTURES.fetch_add(1, Ordering::SeqCst);
}

pub(crate) fn uninterrupted(available: bool, admitted: u64, current: u64) -> bool {
    available && admitted == current
}

pub(crate) struct ForegroundMonitor {
    #[cfg(windows)]
    hook: HWINEVENTHOOK,
    // The hook must be registered and removed on the owning message-loop thread.
    _ui_thread: std::marker::PhantomData<std::rc::Rc<()>>,
}

impl ForegroundMonitor {
    pub(crate) fn new() -> Self {
        Self {
            #[cfg(windows)]
            // SAFETY: static non-unwinding callback; winit pumps this owning UI thread.
            hook: unsafe { SetWinEventHook(
                EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_FOREGROUND, None,
                Some(observe_external_foreground), 0, 0,
                WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
            ) },
            _ui_thread: std::marker::PhantomData,
        }
    }

    pub(crate) fn available(&self) -> bool {
        #[cfg(windows)]
        {
            !self.hook.0.is_null()
        }
        #[cfg(not(windows))]
        {
            true
        }
    }

    pub(crate) fn epoch(&self) -> u64 {
        #[cfg(windows)]
        {
            DEPARTURES.load(Ordering::SeqCst)
        }
        #[cfg(not(windows))]
        {
            0
        }
    }
}

#[cfg(windows)]
impl Drop for ForegroundMonitor {
    fn drop(&mut self) {
        if self.available() {
            // SAFETY: this non-Send owner drops on the same winit UI thread.
            let _ = unsafe { UnhookWinEvent(self.hook) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_departure_invalidates_the_request_even_after_foreground_returns() {
        assert!(uninterrupted(true, 7, 7)); // main -> own confirmation does not increment
        assert!(!uninterrupted(true, 7, 8)); // external focus arrived; current focus may be ours
        assert!(!uninterrupted(false, 7, 7)); // registration failure is fail-closed
        assert!(uninterrupted(true, 8, 8)); // new explicit request, not a revived old approval
    }
}

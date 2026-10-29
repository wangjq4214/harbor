#[path = "tests/architecture.rs"]
mod architecture;

use super::*;

#[test]
fn shared_gpu_exit_cancels_pending_actions_before_exit() {
    use harbor_widget::winit::{GpuFault, GpuFaultKind};
    for kind in [
        GpuFaultKind::OutOfMemory,
        GpuFaultKind::DeviceLost,
        GpuFaultKind::Validation,
        GpuFaultKind::Internal,
    ] {
        let gate = Arc::new(AtomicBool::new(true));
        let mut paste = PasteController::new(gate.clone());
        let mut clipboard =
            crate::clipboard::ClipboardPolicy::new(harbor_config::Osc52WritePolicy::Confirm);
        clipboard.retain(harbor_app::tab_manager::SessionClipboardWrite {
            source: harbor_app::tab_manager::TabId(7),
            contents: harbor_terminal::ClipboardWrite::new("never approve".into()).unwrap(),
        });
        let mut window = None;
        let mut exits = 0;
        apply_gpu_fatal_policy(
            &GpuFault {
                kind,
                message: "policy sentinel".into(),
            },
            &mut paste,
            &mut clipboard,
            &mut window,
            || {
                assert!(!gate.load(std::sync::atomic::Ordering::Acquire));
                exits += 1;
            },
        );
        assert_eq!(exits, 1);
        assert!(!paste.is_active());
        assert!(
            clipboard.resolve(true, true).is_none(),
            "late approval cannot write clipboard"
        );
    }
}

#[test]
fn presented_frame_shows_only_a_pending_window() {
    let outcome = HostFrameOutcome::Presented { wait: None };
    assert_eq!(
        main_frame_policy(&outcome, true),
        MainFramePolicy {
            presented: true,
            show_window: true,
            schedule_hidden_retry: false,
            fatal: false,
        }
    );
    assert!(!main_frame_policy(&outcome, false).show_window);
}

#[test]
fn skipped_hidden_frame_retries_without_showing() {
    let outcome = HostFrameOutcome::Skipped { wait: None };
    let policy = main_frame_policy(&outcome, true);
    assert!(!policy.presented);
    assert!(!policy.show_window);
    assert!(policy.schedule_hidden_retry);
    assert!(!policy.fatal);
}

#[test]
fn recovery_waits_for_host_and_fatal_is_delegated_to_shell() {
    let recovery = HostFrameOutcome::RecoveryScheduled { wait: None };
    assert_eq!(
        main_frame_policy(&recovery, true),
        MainFramePolicy::default()
    );

    let fatal = HostFrameOutcome::Fatal {
        error: harbor_widget::winit::FrameError::out_of_memory(),
        wait: None,
    };
    assert!(main_frame_policy(&fatal, true).fatal);
}

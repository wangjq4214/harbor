#[path = "tests/native.rs"]
mod native;

use super::*;
use std::sync::atomic::AtomicUsize;

fn state() -> (Arc<FaultState>, Arc<AtomicUsize>) {
    let wakes = Arc::new(AtomicUsize::new(0));
    let count = wakes.clone();
    (
        Arc::new(FaultState {
            first: Mutex::new(None),
            wake: GpuWake(Arc::new(move || {
                count.fetch_add(1, Ordering::SeqCst);
            })),
        }),
        wakes,
    )
}

#[test]
fn categories_diagnostics_and_expected_destruction() {
    let (state, wakes) = state();
    state.lost(wgpu::DeviceLostReason::Destroyed, "normal teardown".into());
    assert_eq!(state.observe(), None);
    assert_eq!(wakes.load(Ordering::SeqCst), 0);
    state.lost(wgpu::DeviceLostReason::Unknown, "driver sentinel".into());
    let fault = state.observe().unwrap();
    assert_eq!(fault.kind, GpuFaultKind::DeviceLost);
    assert!(fault.message.contains("driver sentinel"));
    for (error, kind) in [
        (
            wgpu::Error::OutOfMemory {
                source: "oom sentinel".into(),
            },
            GpuFaultKind::OutOfMemory,
        ),
        (
            wgpu::Error::Validation {
                source: "source".into(),
                description: "validation sentinel".into(),
            },
            GpuFaultKind::Validation,
        ),
        (
            wgpu::Error::Internal {
                source: "source".into(),
                description: "internal sentinel".into(),
            },
            GpuFaultKind::Internal,
        ),
    ] {
        let (state, _) = self::state();
        state.uncaptured(error);
        let fault = state.observe().unwrap();
        assert_eq!(fault.kind, kind);
        assert!(fault.message.contains("sentinel"));
    }
}

#[test]
fn racing_notifications_preserve_first_fault_for_every_observer() {
    let (state, wakes) = state();
    let workers: Vec<_> = (0..8)
        .map(|i| {
            let state = state.clone();
            std::thread::spawn(move || state.lost(wgpu::DeviceLostReason::Unknown, i.to_string()))
        })
        .collect();
    for worker in workers {
        worker.join().unwrap();
    }
    let secondary = state.clone();
    let first = secondary.observe().unwrap();
    drop(secondary);
    assert_eq!(state.observe(), Some(first));
    assert_eq!(wakes.load(Ordering::SeqCst), 1);
}

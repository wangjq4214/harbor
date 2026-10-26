//! Device-lifetime notification delivery. No UI or generation-owned callbacks live here.
use std::sync::{
    Arc, Mutex, Weak,
    atomic::{AtomicBool, Ordering},
};
use std::thread::JoinHandle;
use std::time::Duration;
use winit::event_loop::EventLoopProxy;

/// A device notification wake, not a redraw request. Observe each relevant Host on receipt.
#[derive(Clone, Copy, Debug)]
pub struct GpuWakeEvent;

/// Stable native transport for a shared Device's notifications.
///
/// Construct in the event-loop owner, not in a reloadable root. Reused Hosts inherit
/// the bootstrap transport. The conversion must be defined in the stable executable.
#[derive(Clone)]
pub struct GpuWake(Arc<dyn Fn() + Send + Sync>);

impl GpuWake {
    pub fn new<T: From<GpuWakeEvent> + Send + 'static>(proxy: EventLoopProxy<T>) -> Self {
        Self(Arc::new(move || {
            // Event-loop shutdown is ordinary cancellation.
            let _ = proxy.send_event(GpuWakeEvent.into());
        }))
    }
}

/// Device-wide failures are distinct from recoverable/local surface failures.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GpuFaultKind {
    OutOfMemory,
    DeviceLost,
    Validation,
    Internal,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GpuFault {
    pub kind: GpuFaultKind,
    pub message: String,
}

impl std::fmt::Display for GpuFault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}: {}", self.kind, self.message)
    }
}

impl std::error::Error for GpuFault {}

struct FaultState {
    first: Mutex<Option<GpuFault>>,
    wake: GpuWake,
}

impl FaultState {
    fn observe(&self) -> Option<GpuFault> {
        self.first.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    fn record(&self, fault: GpuFault) {
        {
            let mut first = self.first.lock().unwrap_or_else(|e| e.into_inner());
            if first.is_some() {
                return;
            }
            *first = Some(fault);
        }
        // Never invoke transport while holding the state lock.
        (self.wake.0)();
    }

    fn lost(&self, reason: wgpu::DeviceLostReason, message: String) {
        if reason != wgpu::DeviceLostReason::Destroyed {
            self.record(GpuFault {
                kind: GpuFaultKind::DeviceLost,
                message: format!("{reason:?}: {message}"),
            });
        }
    }

    fn uncaptured(&self, error: wgpu::Error) {
        let kind = match &error {
            wgpu::Error::OutOfMemory { .. } => GpuFaultKind::OutOfMemory,
            wgpu::Error::Validation { .. } => GpuFaultKind::Validation,
            wgpu::Error::Internal { .. } => GpuFaultKind::Internal,
        };
        self.record(GpuFault {
            kind,
            // Debug includes the source chain as well as the backend description.
            message: format!("{error:?}"),
        });
    }
}

/// Installed before any surface/renderer/borrowed-device use. Dropped before SharedGpu's
/// device/queue fields. Worker and callbacks never own this monitor or SharedGpu.
pub(super) struct GpuMonitor {
    state: Arc<FaultState>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl GpuMonitor {
    pub(super) fn start(device: &wgpu::Device, wake: GpuWake) -> std::io::Result<Self> {
        let state = Arc::new(FaultState {
            first: Mutex::new(None),
            wake,
        });
        install_callbacks(device, Arc::downgrade(&state));
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        let worker_state = Arc::downgrade(&state);
        let device = device.clone();
        let worker = std::thread::Builder::new()
            .name("harbor-gpu-notifications".into())
            .spawn(move || {
                while !worker_stop.load(Ordering::Acquire) {
                    // DX12's wgpu-hal fence query returns UINT64_MAX on removal without
                    // classifying it as loss. Read the native removal status as well.
                    if let Some(fault) = native_device_fault(&device)
                        && let Some(state) = worker_state.upgrade()
                    {
                        state.record(fault);
                    }
                    // Maintain fence progress and fire deferred wgpu callbacks without
                    // waiting for completion, acquiring a surface or submitting work.
                    if let Err(error) = device.poll(wgpu::PollType::Poll)
                        && let Some(state) = worker_state.upgrade()
                    {
                        state.record(GpuFault {
                            kind: GpuFaultKind::Internal,
                            message: format!("GPU notification poll failed: {error}"),
                        });
                    }
                    std::thread::park_timeout(Duration::from_millis(50));
                }
            })?;
        Ok(Self {
            state,
            stop,
            worker: Some(worker),
        })
    }

    pub(super) fn fault(&self) -> Option<GpuFault> {
        self.state.observe()
    }
}

fn native_device_fault(device: &wgpu::Device) -> Option<GpuFault> {
    #[cfg(target_os = "windows")]
    {
        // SAFETY: read-only COM status query while the HAL guard keeps the device
        // alive. No raw resources are destroyed/mutated and no handles escape.
        let removed = unsafe {
            device
                .as_hal::<wgpu::hal::api::Dx12>()
                .and_then(|hal| hal.raw_device().GetDeviceRemovedReason().err())
        };
        if let Some(error) = removed {
            return Some(GpuFault {
                kind: GpuFaultKind::DeviceLost,
                message: format!("DX12 device removed: {error}"),
            });
        }
    }
    #[cfg(not(target_os = "windows"))]
    let _ = device;
    None
}

fn install_callbacks(device: &wgpu::Device, state: Weak<FaultState>) {
    let lost = state.clone();
    device.set_device_lost_callback(move |reason, message| {
        if let Some(state) = lost.upgrade() {
            state.lost(reason, message);
        }
    });
    device.on_uncaptured_error(Arc::new(move |error| {
        if let Some(state) = state.upgrade() {
            state.uncaptured(error);
        }
    }));
}

impl Drop for GpuMonitor {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            worker.thread().unpark();
            // No panicking during teardown, including a backend worker panic.
            if worker.join().is_err() {
                tracing::error!("GPU notification worker panicked");
            }
        }
        // Weak device callbacks cannot retain the transport or a released generation.
    }
}

#[cfg(test)]
mod tests {
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
                std::thread::spawn(move || {
                    state.lost(wgpu::DeviceLostReason::Unknown, i.to_string())
                })
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

    #[test]
    #[ignore = "requires a native GPU; exercises real callbacks and independent poll progress"]
    fn production_callbacks_and_idle_progress() {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: super::super::gpu::selected_backends(),
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
        eprintln!("notification test adapter: {:?}", adapter.get_info());
        let (normal_device, normal_queue) =
            pollster::block_on(adapter.request_device(&Default::default())).unwrap();
        let (normal_tx, normal_rx) = std::sync::mpsc::channel();
        let normal = GpuMonitor::start(
            &normal_device,
            GpuWake(Arc::new(move || {
                let _ = normal_tx.send(());
            })),
        )
        .unwrap();
        normal_device.destroy();
        normal_device.poll(wgpu::PollType::Poll).unwrap();
        assert!(
            normal.fault().is_none(),
            "normal destroy is not device loss"
        );
        assert!(matches!(
            normal_rx.try_recv(),
            Err(std::sync::mpsc::TryRecvError::Empty)
        ));
        drop(normal);
        drop(normal_queue);
        drop(normal_device);

        let (device, queue) =
            pollster::block_on(adapter.request_device(&Default::default())).unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        let monitor = GpuMonitor::start(
            &device,
            GpuWake(Arc::new(move || {
                let _ = tx.send(());
            })),
        )
        .unwrap();

        // A map callback needs native poll progress. No UI/render loop calls poll.
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("idle notification progress"),
            size: 4,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let (mapped, mapping) = std::sync::mpsc::channel();
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                let _ = mapped.send(result);
            });
        mapping
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .unwrap();
        buffer.unmap();
        assert_eq!(monitor.fault(), None);

        // Harmless real uncaptured validation, through the installed production handler.
        let _ = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("fatal validation sentinel"),
            size: 4,
            usage: wgpu::BufferUsages::empty(),
            mapped_at_creation: false,
        });
        rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let fault = monitor.fault().unwrap();
        assert_eq!(fault.kind, GpuFaultKind::Validation);
        assert!(fault.message.contains("fatal validation sentinel"));
        assert_eq!(monitor.fault(), Some(fault));
        let weak = Arc::downgrade(&monitor.state);
        drop(monitor);
        assert!(weak.upgrade().is_none());
        drop(rx); // closed transport/late device callbacks cannot panic
        for _ in 0..8 {
            let (tx, rx) = std::sync::mpsc::channel();
            let monitor = GpuMonitor::start(
                &device,
                GpuWake(Arc::new(move || {
                    let _ = tx.send(());
                })),
            )
            .unwrap();
            let weak = Arc::downgrade(&monitor.state);
            drop(rx);
            let producer_device = device.clone();
            let producer = std::thread::spawn(move || {
                let _ = producer_device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("teardown race"),
                    size: 4,
                    usage: wgpu::BufferUsages::empty(),
                    mapped_at_creation: false,
                });
            });
            drop(monitor);
            producer.join().unwrap();
            assert!(weak.upgrade().is_none());
        }
        drop(queue);
        device.destroy();
        let _ = device.poll(wgpu::PollType::Poll);
    }
}

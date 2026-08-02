#![cfg(feature = "winit")]

use harbor_widget::effects::RuntimeEffects;
use harbor_widget::renderer::Viewport;
use harbor_widget::runtime::Runtime;
use harbor_widget::winit::{FrameError, FrameOutcome, WinitAdapter, WinitFrameTarget};
use winit::event::WindowEvent;
use winit::window::Window;

// This fixture is type-checked without constructing an OS window or GPU
// surface. In particular, the target owns no host resources.
fn borrowed_frame_contract<'frame, 'surface>(
    window: &'frame Window,
    surface: &'frame wgpu::Surface<'surface>,
    device: &'frame wgpu::Device,
    queue: &'frame wgpu::Queue,
    config: &'frame mut wgpu::SurfaceConfiguration,
    event: &WindowEvent,
) {
    let target = WinitFrameTarget::new(
        window,
        surface,
        device,
        queue,
        config,
        Viewport::new(800, 600, 1.0),
        wgpu::Color::BLACK,
    );
    assert_eq!(target.viewport().physical_size, (800, 600));
    let _ = target.window();
    let _ = target.surface();
    let _ = target.device();
    let _ = target.queue();
    let _ = target.config();
    let _ = target.clear_color();

    let mut runtime = Runtime::new();
    let mut adapter = WinitAdapter::new();
    let effects: RuntimeEffects = adapter.handle_event(&mut runtime, event);
    assert!(effects.is_noop());
    assert_eq!(adapter.render(&mut runtime, target), FrameOutcome::Deferred);
}

#[test]
fn adapters_have_independent_state() {
    let first = WinitAdapter::new();
    let second = WinitAdapter::new();
    assert_ne!(&first as *const _, &second as *const _);
}

#[test]
fn frame_outcomes_classify_fatal_and_nonfatal_results() {
    assert!(FrameOutcome::presented().is_presented());
    assert!(!FrameOutcome::skipped().is_fatal());
    assert!(!FrameOutcome::deferred().is_fatal());
    assert!(FrameOutcome::recovery_required().is_recovery_required());

    let outcome = FrameOutcome::fatal(FrameError::out_of_memory());
    assert!(outcome.is_fatal());
    assert_eq!(outcome, FrameOutcome::Fatal(FrameError::OutOfMemory));
}

#[test]
fn frame_error_categories_are_host_inspectable() {
    assert_eq!(FrameError::device_lost(), FrameError::DeviceLost);
    assert_eq!(
        FrameError::validation("invalid pass"),
        FrameError::Validation("invalid pass".into())
    );
    assert_eq!(
        FrameError::presentation("surface"),
        FrameError::Presentation("surface".into())
    );
    assert_eq!(
        FrameError::other("unexpected"),
        FrameError::Other("unexpected".into())
    );
}

#[allow(dead_code)]
fn compile_only_borrowed_frame_contract() {
    let _ = borrowed_frame_contract;
}

//! Adapter-owned native window, runtime, surface, and presentation lifecycle.

#[cfg(feature = "hmr")]
use super::WidgetHmrWork;
use super::effects::apply_window_effects;
#[cfg(feature = "hmr")]
use super::hmr::{WidgetHmrConfig, WidgetHmrRoot, WidgetHmrState, WidgetHmrWorkKind};
use super::{
    FrameError, FrameOutcome, SharedGpu, WindowPresenter, WindowSurface, WinitAdapter,
    WinitFrameTarget,
};
use crate::effects::{ClipboardEffect, ControlFlowEffect, ExternalInvalidation, RuntimeEffects};
use crate::input::event::UiEvent;
use crate::renderer::Viewport;
use crate::scene::primitive::ExternalDrawId;
use crate::text::TextMetrics;
use crate::view::Component;
use crate::widgets::FocusHandle;
use anyhow::Context as _;
use std::any::Any;
use std::fmt;
use std::sync::Arc;
use std::time::Instant;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::ModifiersState;
use winit::window::{Window, WindowAttributes, WindowId};

/// Identifies the construction stage that failed before a host became usable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostStartupStage {
    ConfigureAttributes,
    CreateWindow,
    PlatformSetup,
    GpuBootstrap,
    SurfaceCreation,
    RootConstruction,
    RuntimeInitialization,
}

/// A startup failure annotated with the resource-construction stage.
#[derive(Debug)]
pub struct HostStartupError {
    stage: HostStartupStage,
    source: anyhow::Error,
}

impl HostStartupError {
    fn new(stage: HostStartupStage, source: impl Into<anyhow::Error>) -> Self {
        Self {
            stage,
            source: source.into(),
        }
    }

    pub const fn stage(&self) -> HostStartupStage {
        self.stage
    }
}

impl fmt::Display for HostStartupError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "native window host {:?} failed: {}",
            self.stage, self.source
        )
    }
}

impl std::error::Error for HostStartupError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.source.as_ref())
    }
}

/// Read-only surface metadata available while constructing the root component.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowSurfaceInfo {
    pub format: wgpu::TextureFormat,
    pub alpha_mode: wgpu::CompositeAlphaMode,
    pub physical_size: (u32, u32),
}

/// Construction-only access to the new window and its compatible GPU resources.
#[derive(Clone, Copy)]
pub struct HostInitContext<'a> {
    window: &'a Window,
    gpu: &'a SharedGpu,
    shared_gpu: &'a Arc<SharedGpu>,
    surface: WindowSurfaceInfo,
    backdrop_available: bool,
    text_metrics: &'a TextMetrics,
}

impl<'a> HostInitContext<'a> {
    pub const fn window(&self) -> &'a Window {
        self.window
    }

    pub const fn gpu(&self) -> &'a SharedGpu {
        self.gpu
    }

    pub const fn shared_gpu(&self) -> &'a Arc<SharedGpu> {
        self.shared_gpu
    }

    pub const fn surface(&self) -> WindowSurfaceInfo {
        self.surface
    }

    /// Returns the final UI text metrics loaded by the Host-owned Runtime.
    pub const fn text_metrics(&self) -> &'a TextMetrics {
        self.text_metrics
    }

    /// Returns the final backdrop decision after the surface alpha mode is known.
    pub const fn backdrop_available(&self) -> bool {
        self.backdrop_available
    }
}

/// Application-provided native setup around window creation.
///
/// The associated setup value is passed to the root factory, then retained as
/// opaque drop-only state for exactly the host lifetime.
pub trait WindowPlatformHooks {
    type Setup: 'static;

    fn configure_attributes(
        &self,
        attributes: WindowAttributes,
    ) -> anyhow::Result<WindowAttributes> {
        Ok(attributes)
    }

    fn window_created(&self, window: &Window) -> anyhow::Result<Self::Setup>;

    /// Finalizes platform setup after surface metadata exists and before root construction.
    fn surface_ready(
        &self,
        _window: &Window,
        _setup: &mut Self::Setup,
        _surface: WindowSurfaceInfo,
    ) -> anyhow::Result<bool> {
        Ok(false)
    }
}

/// Default platform lifecycle seam with no application-specific setup.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoopWindowPlatformHooks;

impl WindowPlatformHooks for NoopWindowPlatformHooks {
    type Setup = ();

    fn window_created(&self, _window: &Window) -> anyhow::Result<Self::Setup> {
        Ok(())
    }
}

/// Selects first-window GPU bootstrap or reuse for a subsequent window.
#[derive(Clone)]
pub enum HostGpuSource {
    Bootstrap,
    Reuse(Arc<SharedGpu>),
}

/// Configures and constructs one complete [`WinitWindowHost`].
pub struct WinitWindowHostBuilder<P, F> {
    attributes: WindowAttributes,
    gpu_source: HostGpuSource,
    platform: P,
    root_factory: F,
    focus_first: bool,
    #[cfg(feature = "hmr")]
    hmr: Option<WidgetHmrConfig>,
}

impl<F> WinitWindowHostBuilder<NoopWindowPlatformHooks, F> {
    pub fn new(attributes: WindowAttributes, root_factory: F) -> Self {
        Self {
            attributes,
            gpu_source: HostGpuSource::Bootstrap,
            platform: NoopWindowPlatformHooks,
            root_factory,
            focus_first: false,
            #[cfg(feature = "hmr")]
            hmr: None,
        }
    }
}

impl<P, F> WinitWindowHostBuilder<P, F> {
    pub fn reuse_gpu(mut self, gpu: Arc<SharedGpu>) -> Self {
        self.gpu_source = HostGpuSource::Reuse(gpu);
        self
    }

    pub fn bootstrap_gpu(mut self) -> Self {
        self.gpu_source = HostGpuSource::Bootstrap;
        self
    }

    pub fn focus_first(mut self, focus_first: bool) -> Self {
        self.focus_first = focus_first;
        self
    }

    pub fn with_platform_hooks<Q>(self, platform: Q) -> WinitWindowHostBuilder<Q, F> {
        WinitWindowHostBuilder {
            attributes: self.attributes,
            gpu_source: self.gpu_source,
            platform,
            root_factory: self.root_factory,
            #[cfg(feature = "hmr")]
            hmr: self.hmr,
            focus_first: self.focus_first,
        }
    }
    #[cfg(feature = "hmr")]
    pub fn with_hmr(mut self, hmr: WidgetHmrConfig) -> Self {
        self.hmr = Some(hmr);
        self
    }
}

impl<P, F> WinitWindowHostBuilder<P, F>
where
    P: WindowPlatformHooks,
{
    pub async fn build<R>(
        self,
        event_loop: &ActiveEventLoop,
    ) -> Result<WinitWindowHost, HostStartupError>
    where
        F: for<'a> FnOnce(HostInitContext<'a>, &'a P::Setup) -> anyhow::Result<R>,
        R: Component + 'static,
    {
        let WinitWindowHostBuilder {
            attributes,
            gpu_source,
            platform,
            root_factory,
            focus_first,
            #[cfg(feature = "hmr")]
            hmr,
        } = self;
        let (host, ()) = WinitWindowHostBuilder {
            attributes,
            gpu_source,
            platform,
            root_factory: move |context: HostInitContext<'_>, setup: &P::Setup| {
                root_factory(context, setup).map(|root| (root, ()))
            },
            #[cfg(feature = "hmr")]
            hmr,
            focus_first,
        }
        .build_with_output(event_loop)
        .await?;
        Ok(host)
    }

    /// Builds a complete host and atomically returns application-owned bootstrap output.
    pub async fn build_with_output<R, A>(
        self,
        event_loop: &ActiveEventLoop,
    ) -> Result<(WinitWindowHost, A), HostStartupError>
    where
        F: for<'a> FnOnce(HostInitContext<'a>, &'a P::Setup) -> anyhow::Result<(R, A)>,
        R: Component + 'static,
    {
        let attributes = self
            .platform
            .configure_attributes(self.attributes)
            .map_err(|error| HostStartupError::new(HostStartupStage::ConfigureAttributes, error))?;
        let window = Arc::new(event_loop.create_window(attributes).map_err(|error| {
            HostStartupError::new(HostStartupStage::CreateWindow, anyhow::Error::new(error))
        })?);
        let mut platform_state = self
            .platform
            .window_created(&window)
            .map_err(|error| HostStartupError::new(HostStartupStage::PlatformSetup, error))?;

        let (gpu, surface) = match self.gpu_source {
            HostGpuSource::Bootstrap => {
                let (gpu, surface) =
                    SharedGpu::new(Arc::clone(&window)).await.map_err(|error| {
                        HostStartupError::new(HostStartupStage::GpuBootstrap, error)
                    })?;
                (Arc::new(gpu), surface)
            }
            HostGpuSource::Reuse(gpu) => {
                let surface = gpu
                    .create_window_surface(Arc::clone(&window))
                    .map_err(|error| {
                        HostStartupError::new(HostStartupStage::SurfaceCreation, error)
                    })?;
                (gpu, surface)
            }
        };

        let surface_info = WindowSurfaceInfo {
            format: surface.format(),
            alpha_mode: surface.alpha_mode(),
            physical_size: window.inner_size().into(),
        };
        let backdrop_available = self
            .platform
            .surface_ready(&window, &mut platform_state, surface_info)
            .map_err(|error| HostStartupError::new(HostStartupStage::PlatformSetup, error))?;
        let mut runtime = crate::runtime::Runtime::new();
        runtime.init_renderer(gpu.device(), surface.format());
        runtime
            .init_text_renderer(gpu.device(), gpu.queue(), surface.format())
            .context("initialize host text renderer")
            .map_err(|error| {
                HostStartupError::new(HostStartupStage::RuntimeInitialization, error)
            })?;
        let adapter = WinitAdapter::from_window(&window);
        let mut presenter = WindowPresenter::from_window(&window);
        let size = window.inner_size();
        let drawable = size.width != 0 && size.height != 0;
        presenter.set_drawable(drawable);
        runtime.set_viewport(presenter.viewport().clone());

        let root_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            (self.root_factory)(
                HostInitContext {
                    window: &window,
                    gpu: gpu.as_ref(),
                    shared_gpu: &gpu,
                    surface: surface_info,
                    backdrop_available,
                    text_metrics: runtime.text_metrics(),
                },
                &platform_state,
            )
        }))
        .map_err(|panic| {
            HostStartupError::new(
                HostStartupStage::RootConstruction,
                anyhow::anyhow!("root factory panicked: {}", panic_message(panic)),
            )
        })?;
        let (root, application_output) = root_result
            .map_err(|error| HostStartupError::new(HostStartupStage::RootConstruction, error))?;

        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| runtime.set_root(root))).map_err(
            |panic| {
                HostStartupError::new(
                    HostStartupStage::RootConstruction,
                    anyhow::anyhow!("root component panicked: {}", panic_message(panic)),
                )
            },
        )?;

        let mut initial_effects = runtime.update(Instant::now());
        if self.focus_first {
            runtime.focus_first_focusable();
            initial_effects.merge(runtime.take_pending_effects());
        }
        let mut initial_effects = presenter.fold_effects(initial_effects);
        initial_effects.merge(presenter.request_frame());
        apply_window_effects(&window, initial_effects);

        #[cfg(feature = "hmr")]
        let hmr = self
            .hmr
            .map(WidgetHmrConfig::start)
            .transpose()
            .map_err(|error| {
                HostStartupError::new(
                    HostStartupStage::RuntimeInitialization,
                    anyhow::Error::new(error).context("start widget hot-reload observer"),
                )
            })?
            .flatten();
        Ok((
            WinitWindowHost {
                runtime,
                surface,
                adapter,
                presenter,
                _platform_keepalive: Box::new(platform_state),
                window,
                gpu,
                backdrop_available,
                #[cfg(feature = "hmr")]
                hmr,
            },
            application_output,
        ))
    }
}

/// The result of a complete host-owned frame attempt.
#[derive(Clone, Debug, PartialEq)]
pub enum HostFrameOutcome {
    Presented {
        wait: Option<ControlFlowEffect>,
    },
    PresentedSuboptimal {
        wait: Option<ControlFlowEffect>,
    },
    Skipped {
        wait: Option<ControlFlowEffect>,
    },
    RecoveryScheduled {
        wait: Option<ControlFlowEffect>,
    },
    Fatal {
        error: FrameError,
        wait: Option<ControlFlowEffect>,
    },
}

impl HostFrameOutcome {
    pub const fn wait(&self) -> Option<ControlFlowEffect> {
        match self {
            Self::Presented { wait }
            | Self::PresentedSuboptimal { wait }
            | Self::Skipped { wait }
            | Self::RecoveryScheduled { wait }
            | Self::Fatal { wait, .. } => *wait,
        }
    }

    pub const fn is_presented(&self) -> bool {
        matches!(
            self,
            Self::Presented { .. } | Self::PresentedSuboptimal { .. }
        )
    }

    pub const fn is_fatal(&self) -> bool {
        matches!(self, Self::Fatal { .. })
    }
}

/// A routed native event, including business input drained at the host boundary.
#[derive(Clone, Debug, PartialEq)]
pub struct HostEventOutcome {
    pub handled: bool,
    pub external_input: Vec<(ExternalDrawId, UiEvent)>,
    pub frame: Option<HostFrameOutcome>,
    pub wait: Option<ControlFlowEffect>,
}

/// A non-frame host turn and its cross-window wait demand.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HostIdleOutcome {
    pub wait: Option<ControlFlowEffect>,
}

/// Owns all native and widget state whose invariants are scoped to one window.
///
/// Field order is intentional: runtime/surface state drops before platform
/// keepalive, then the native window, and finally the shared GPU reference.
pub struct WinitWindowHost {
    runtime: crate::runtime::Runtime,
    surface: WindowSurface,
    presenter: WindowPresenter,
    adapter: WinitAdapter,
    _platform_keepalive: Box<dyn Any>,
    window: Arc<Window>,
    gpu: Arc<SharedGpu>,
    backdrop_available: bool,
    #[cfg(feature = "hmr")]
    hmr: Option<WidgetHmrState>,
}

impl WinitWindowHost {
    pub fn window(&self) -> &Window {
        &self.window
    }

    pub fn window_id(&self) -> WindowId {
        self.window.id()
    }

    pub fn gpu(&self) -> &SharedGpu {
        &self.gpu
    }

    pub fn shared_gpu(&self) -> &Arc<SharedGpu> {
        &self.gpu
    }

    pub fn viewport(&self) -> &Viewport {
        self.presenter.viewport()
    }

    pub fn modifiers(&self) -> ModifiersState {
        self.adapter.modifiers()
    }

    pub const fn backdrop_available(&self) -> bool {
        self.backdrop_available
    }

    /// Records a pointer activation rejected by an application-owned modal gate.
    pub fn quarantine_blocked_pointer_event(&mut self, event: &WindowEvent) {
        self.adapter.quarantine_blocked_pointer_event(event);
    }

    /// Cancels all input ownership before replacing or gating an input subtree.
    pub fn cancel_active_input_ownership(&mut self) -> HostIdleOutcome {
        self.adapter.quarantine_active_pointers();
        let mut effects = self
            .runtime
            .cancel_pointer_captures(crate::layout::Point::ZERO);
        effects.merge(self.runtime.take_pending_effects());
        self.apply_runtime_effects(effects)
    }

    /// Requests focus for one stable application-owned focus target.
    pub fn request_focus(&mut self, handle: &FocusHandle) -> HostIdleOutcome {
        let mut effects = self.runtime.request_focus(handle);
        effects.merge(self.runtime.take_pending_effects());
        self.apply_runtime_effects(effects)
    }

    /// Removes focus without exposing the underlying runtime.
    pub fn clear_focus(&mut self) -> HostIdleOutcome {
        self.runtime.clear_focus();
        let effects = self.runtime.take_pending_effects();
        self.apply_runtime_effects(effects)
    }

    /// Applies a command-generated clipboard write through the native effect boundary.
    pub fn write_clipboard(&mut self, text: impl Into<String>) -> HostIdleOutcome {
        self.apply_runtime_effects(RuntimeEffects {
            clipboard: Some(ClipboardEffect::write(text)),
            ..RuntimeEffects::default()
        })
    }

    /// Applies one opaque adapter-owned hot-reload work item on the UI thread.
    #[cfg(feature = "hmr")]
    pub fn handle_hmr_work(&mut self, work: WidgetHmrWork) -> HostIdleOutcome {
        if self.hmr.is_none() {
            return HostIdleOutcome::default();
        }

        match work.into_kind() {
            WidgetHmrWorkKind::Prepare {
                generation,
                barrier,
            } => {
                let accepted = self
                    .hmr
                    .as_mut()
                    .is_some_and(|hmr| hmr.lifecycle.begin_prepare(generation));
                if !accepted {
                    tracing::warn!(generation, "ignored stale widget HMR prepare work");
                    drop(barrier);
                    return HostIdleOutcome::default();
                }

                self.adapter.quarantine_active_pointers();
                let mut effects = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    self.runtime
                        .cancel_pointer_captures(crate::layout::Point::ZERO)
                }))
                .unwrap_or_else(|panic| {
                    tracing::error!(
                        generation,
                        error = %panic_message(panic),
                        "widget input cancellation panicked during HMR prepare"
                    );
                    RuntimeEffects::default()
                });
                self.runtime.clear_root();
                effects.merge(self.runtime.update(Instant::now()));
                effects.merge(self.runtime.take_pending_effects());
                let outcome = self.apply_runtime_effects(effects);
                // The old library may unload only after every Runtime-owned reference is gone.
                drop(barrier);
                tracing::info!(generation, "prepared widget host for hot reload");
                outcome
            }
            WidgetHmrWorkKind::Activate { generation } => {
                let accepted = self
                    .hmr
                    .as_mut()
                    .is_some_and(|hmr| hmr.lifecycle.begin_activate(generation));
                if !accepted {
                    tracing::warn!(
                        generation,
                        "ignored stale or duplicate widget HMR activation"
                    );
                    return HostIdleOutcome::default();
                }

                let installed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let root = {
                        let hmr = self.hmr.as_ref().expect("HMR state checked above");
                        (hmr.root_factory)()
                    };
                    self.runtime.set_root(WidgetHmrRoot(root));
                    let mut effects = self.runtime.update(Instant::now());
                    effects.merge(self.runtime.take_pending_effects());
                    effects
                }));
                let mut effects = match installed {
                    Ok(effects) => effects,
                    Err(panic) => {
                        tracing::error!(
                            generation,
                            error = %panic_message(panic),
                            "widget HMR root activation failed"
                        );
                        self.runtime.clear_root();
                        let effects = self.runtime.update(Instant::now());
                        return self.apply_runtime_effects(effects);
                    }
                };
                effects = self.presenter.fold_effects(effects);
                effects.merge(self.presenter.request_frame());
                self.hmr
                    .as_mut()
                    .expect("HMR state checked above")
                    .lifecycle
                    .finish_activate(generation);
                tracing::info!(generation, "activated widget HMR generation");
                HostIdleOutcome {
                    wait: apply_window_effects(&self.window, effects),
                }
            }
        }
    }

    pub fn invalidate_external(&mut self, work: ExternalInvalidation) -> HostIdleOutcome {
        let mut effects = self.presenter.invalidate_external(&mut self.runtime, work);
        let update = self
            .presenter
            .fold_effects(self.runtime.update(Instant::now()));
        effects.merge(update);
        HostIdleOutcome {
            wait: apply_window_effects(&self.window, effects),
        }
    }

    pub fn request_frame(&mut self) -> HostIdleOutcome {
        let effects = self.presenter.request_frame();
        HostIdleOutcome {
            wait: apply_window_effects(&self.window, effects),
        }
    }

    /// Immediately attempts one frame while retaining presentation policy in the caller.
    pub fn present_now(&mut self) -> HostFrameOutcome {
        self.redraw()
    }

    pub fn about_to_wait(
        &mut self,
        now: Instant,
        host_deadline: Option<Instant>,
    ) -> HostIdleOutcome {
        let effects = self
            .presenter
            .about_to_wait(&mut self.runtime, now, host_deadline);
        HostIdleOutcome {
            wait: apply_window_effects(&self.window, effects),
        }
    }

    pub fn handle_window_event(&mut self, event: &WindowEvent) -> HostEventOutcome {
        if matches!(event, WindowEvent::RedrawRequested) {
            let frame = self.redraw();
            return HostEventOutcome {
                handled: true,
                external_input: self.runtime.drain_external_input(),
                wait: frame.wait(),
                frame: Some(frame),
            };
        }

        #[cfg(feature = "hmr")]
        if self
            .hmr
            .as_ref()
            .is_some_and(|hmr| !hmr.lifecycle.accepts_widget_input())
            && !matches!(
                event,
                WindowEvent::Resized(_)
                    | WindowEvent::ScaleFactorChanged { .. }
                    | WindowEvent::CloseRequested
                    | WindowEvent::Focused(_)
                    | WindowEvent::CursorLeft { .. }
            )
        {
            self.adapter.observe_blocked_widget_event(event);
            return HostEventOutcome {
                handled: true,
                external_input: Vec::new(),
                frame: None,
                wait: None,
            };
        }

        let size = self.window.inner_size();
        let outcome = self.presenter.handle_event_with_size(
            &mut self.adapter,
            &mut self.runtime,
            event,
            Some((size.width, size.height)),
        );
        HostEventOutcome {
            handled: outcome.handled,
            external_input: self.runtime.drain_external_input(),
            frame: None,
            wait: apply_window_effects(&self.window, outcome.effects),
        }
    }

    fn apply_runtime_effects(&mut self, effects: RuntimeEffects) -> HostIdleOutcome {
        let effects = self.presenter.fold_effects(effects);
        HostIdleOutcome {
            wait: apply_window_effects(&self.window, effects),
        }
    }

    fn redraw(&mut self) -> HostFrameOutcome {
        let target = WinitFrameTarget::new(&self.window, &self.gpu, &mut self.surface);
        let frame = self
            .presenter
            .render_with_prepare(&mut self.runtime, target, |runtime| {
                runtime.prepare_text(self.gpu.queue());
            });
        self.finish_frame(frame)
    }

    fn finish_frame(&self, frame: FrameOutcome) -> HostFrameOutcome {
        finish_frame_with(frame, |effects| apply_window_effects(&self.window, effects))
    }
}

fn panic_message(panic: Box<dyn Any + Send>) -> String {
    if let Some(message) = panic.downcast_ref::<&str>() {
        (*message).to_owned()
    } else if let Some(message) = panic.downcast_ref::<String>() {
        message.clone()
    } else {
        "unknown panic payload".to_owned()
    }
}

fn finish_frame_with(
    frame: FrameOutcome,
    mut apply_effects: impl FnMut(RuntimeEffects) -> Option<ControlFlowEffect>,
) -> HostFrameOutcome {
    match frame {
        FrameOutcome::Presented(effects) => HostFrameOutcome::Presented {
            wait: apply_effects(effects),
        },
        FrameOutcome::PresentedSuboptimal(effects) => HostFrameOutcome::PresentedSuboptimal {
            wait: apply_effects(effects),
        },
        FrameOutcome::Skipped(effects) => HostFrameOutcome::Skipped {
            wait: apply_effects(effects),
        },
        FrameOutcome::RecoveryRequired(effects) => HostFrameOutcome::RecoveryScheduled {
            wait: apply_effects(effects),
        },
        FrameOutcome::Fatal(error, effects) => HostFrameOutcome::Fatal {
            error,
            wait: apply_effects(effects),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_outcomes_keep_wait_visible_without_runtime_effects() {
        let deadline = Instant::now();
        let outcome = HostFrameOutcome::RecoveryScheduled {
            wait: Some(ControlFlowEffect::WaitUntil(deadline)),
        };
        assert_eq!(outcome.wait(), Some(ControlFlowEffect::WaitUntil(deadline)));
        assert!(!outcome.is_presented());
        assert!(!outcome.is_fatal());
    }

    #[test]
    fn frame_mapping_consumes_effects_once_for_every_disposition() {
        let cases = [
            FrameOutcome::presented(RuntimeEffects::request_redraw()),
            FrameOutcome::presented_suboptimal(RuntimeEffects::request_redraw()),
            FrameOutcome::skipped(RuntimeEffects::request_redraw()),
            FrameOutcome::recovery_required(RuntimeEffects::request_redraw()),
            FrameOutcome::fatal(
                FrameError::out_of_memory(),
                RuntimeEffects::request_redraw(),
            ),
        ];

        for frame in cases {
            let mut applied = 0;
            let outcome = finish_frame_with(frame, |effects| {
                applied += 1;
                assert!(effects.request_redraw);
                Some(ControlFlowEffect::Poll)
            });
            assert_eq!(applied, 1);
            assert_eq!(outcome.wait(), Some(ControlFlowEffect::Poll));
        }
    }

    #[test]
    fn startup_errors_preserve_stage_and_source_context() {
        for stage in [
            HostStartupStage::ConfigureAttributes,
            HostStartupStage::CreateWindow,
            HostStartupStage::PlatformSetup,
            HostStartupStage::GpuBootstrap,
            HostStartupStage::SurfaceCreation,
            HostStartupStage::RootConstruction,
            HostStartupStage::RuntimeInitialization,
        ] {
            let error = HostStartupError::new(stage, anyhow::anyhow!("sentinel failure"));
            assert_eq!(error.stage(), stage);
            assert!(error.to_string().contains("sentinel failure"));
            assert!(std::error::Error::source(&error).is_some());
        }
    }

    #[test]
    fn gpu_source_expresses_bootstrap_and_reuse_at_the_type_boundary() {
        fn accepts(_: HostGpuSource) {}
        accepts(HostGpuSource::Bootstrap);
        let _ = accepts;
    }
}

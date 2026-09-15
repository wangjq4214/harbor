//! Application-specific glue for the debug-reloadable UI library.

use harbor_app::tab_view::ui::MainWindowRootInputs;
use harbor_widget::view::Component;
use harbor_widget::winit::{WidgetHmrConfig, WidgetHmrWork};

#[hot_lib_reloader::hot_module(
    dylib = "harbor_app_ui",
    loaded_lib_name_template = "{lib_name}_hot_{pid}_{load_counter}"
)]
mod hot_ui {
    use harbor_app::tab_view::ui::MainWindowRootInputs;
    use harbor_widget::view::Component;

    hot_functions_from_file!("crates/harbor-app-ui/src/lib.rs");

    #[lib_change_subscription]
    pub fn subscribe() -> hot_lib_reloader::LibReloadObserver {}
}

pub(crate) fn build_root(inputs: MainWindowRootInputs) -> Box<dyn Component> {
    hot_ui::build_root(inputs)
}

/// Binds this application's dynamic-library identity to the widget-owned HMR lifecycle.
pub(crate) fn config<W, F>(wake: W, root_factory: F) -> Result<WidgetHmrConfig, String>
where
    W: Fn(WidgetHmrWork) -> bool + Send + 'static,
    F: Fn() -> Box<dyn Component> + 'static,
{
    let observer = std::panic::catch_unwind(hot_ui::subscribe)
        .map_err(|_| "load initial reloadable UI library".to_owned())?;
    Ok(WidgetHmrConfig::new(observer, wake, root_factory))
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{Arc, Mutex, atomic::AtomicBool},
        time::Instant,
    };

    use harbor_app::{
        tab_manager::{TabManager, TerminalTabResources},
        tab_view::{TabCommand, TabUiController, ui::MainWindowRootInputs},
        terminal_view::TerminalWidgetBridge,
    };
    use harbor_terminal::Terminal;
    use harbor_widget::{
        effects::ExternalInvalidation,
        input::event::{Key, KeyboardEvent, Modifiers, UiEvent},
        renderer::Viewport,
        runtime::Runtime,
        view::{BuildCx, Component, View},
    };

    use super::build_root;

    struct MountedRoot(Box<dyn Component>);

    impl Component for MountedRoot {
        fn build(&self, cx: &mut BuildCx) -> View {
            self.0.build(cx)
        }
    }

    #[test]
    #[allow(clippy::arc_with_non_send_sync)]
    fn dynamic_generation_accepts_host_contract_and_builds_a_view() {
        let mut tabs = TabManager::new();
        tabs.create_tab(|_, draw_id| {
            let terminal = Arc::new(Mutex::new(Terminal::new_headless(4, 20)));
            let bridge = TerminalWidgetBridge::new(
                draw_id,
                Arc::clone(&terminal),
                Arc::new(AtomicBool::new(false)),
            );
            Ok(TerminalTabResources::new(terminal, bridge))
        })
        .expect("create reload smoke-test tab");
        let controller = TabUiController::new(
            tabs.snapshots(),
            tabs.active_bridge().expect("active bridge"),
            1_200.0,
        );

        let published = controller.clone();
        let root = build_root(MainWindowRootInputs::new(controller, false, [0.0; 3]));
        let mut runtime = Runtime::new();
        runtime.set_viewport(Viewport::new(1_200, 600, 1.0));
        runtime.set_root(MountedRoot(root));
        let _ = runtime.update(Instant::now());

        runtime.dispatch(UiEvent::Keyboard(KeyboardEvent::KeyDown {
            key: Key::Character('t'),
            modifiers: Modifiers {
                ctrl: true,
                ..Modifiers::default()
            },
        }));
        let actions = published.drain_actions();
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0].command, TabCommand::New));
        assert!(published.drain_actions().is_empty());

        assert!(published.update_presentation(500.0));
        let mut effects = runtime.invalidate_external(ExternalInvalidation::new());
        effects.merge(runtime.update(Instant::now()));
        assert!(effects.request_redraw);
    }
}

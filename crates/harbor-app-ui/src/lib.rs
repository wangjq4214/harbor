//! Debug-reloadable application widget composition boundary.

use harbor_app::tab_view::ui::{MainWindowRootInputs, main_window_root};
use harbor_widget::view::Component;

/// Builds a fresh application root for the active dynamic-library generation.
///
/// The Host must drop the returned component and every View it built before this
/// library generation is unloaded.
#[unsafe(no_mangle)]
pub fn build_root(inputs: MainWindowRootInputs) -> Box<dyn Component> {
    Box::new(main_window_root(inputs))
}

//! Debug-reloadable application widget composition boundary.

use harbor_app::tab_view::TabUiController;
use harbor_widget::view::Component;

/// Builds a fresh application root for the active dynamic-library generation.
///
/// The Host must drop the returned component and every View it built before this
/// library generation is unloaded.
#[unsafe(no_mangle)]
pub fn build_root(
    controller: TabUiController,
    backdrop_available: bool,
    backdrop_fallback: [f32; 3],
) -> Box<dyn Component> {
    Box::new(harbor_app::tab_view::ui::tab_workspace_with_fallback(
        controller,
        backdrop_available,
        backdrop_fallback,
    ))
}

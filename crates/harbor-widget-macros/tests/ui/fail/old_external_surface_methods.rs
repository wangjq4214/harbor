use harbor_widget::{
    ExternalSurface,
    scene::primitive::{ExternalDrawFn, ExternalImeFn, ExternalScheduleDemand, ExternalScheduleFn},
};
use std::sync::Arc;

fn main() {
    let draw: Arc<ExternalDrawFn<'static>> = Arc::new(|_, _, _, _, _| {});
    let schedule: Arc<ExternalScheduleFn> = Arc::new(|_, _| ExternalScheduleDemand::empty());
    let ime: Arc<ExternalImeFn> = Arc::new(|_, _| None);
    let _ = ExternalSurface::new(1).handler(draw);
    let _ = ExternalSurface::new(1).schedule(schedule);
    let _ = ExternalSurface::new(1).ime(ime);
}

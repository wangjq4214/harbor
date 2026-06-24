/// Common interface for render stages that draw into a target.
pub(crate) trait Render<Target = ()> {
    fn render(&mut self, target: Target);
}

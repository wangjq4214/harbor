use std::num::NonZeroU32;

/// Metadata consumed only by a view's immediate layout parent.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ParentData {
    pub flex: Option<FlexParentData>,
}

/// A positive weight and allocation policy for a flexible child.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FlexParentData {
    pub factor: NonZeroU32,
    pub fit: FlexFit,
}

/// Whether flexible content may under-consume its allocated main-axis share.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlexFit {
    Loose,
    Tight,
}

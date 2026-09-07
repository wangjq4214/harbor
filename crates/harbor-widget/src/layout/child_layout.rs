use super::{BoxConstraints, ParentData, Point, Size};

/// Recoverable layout conditions retained alongside committed geometry.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LayoutDiagnostic {
    UnboundedFlex,
    InvalidFlexParent,
    ConflictingFlexParentData,
    Overflow { main: f32, cross: f32 },
}

/// A layout contract violation. A failed candidate never partially commits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayoutError {
    InvalidConstraints,
    InvalidGeometry,
    InvalidChildIndex,
    MissingPlacement,
    DuplicatePlacement,
    UnmeasuredChild,
    /// A natural-size probe was selected without a final measurement.
    UnfinalizedMeasurement,
    MeasurementLimit,
    WorkBudgetExceeded,
    RecursiveMeasurement,
    StaleChild,
    /// A malformed fiber graph references the same child more than once.
    DuplicateChild,
}

impl std::fmt::Display for LayoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "layout contract violation: {self:?}")
    }
}

impl std::error::Error for LayoutError {}

/// Restricted access to immediate children during one parent invocation.
///
/// Identical constraints and phase reuse a cached subtree and select it for final
/// placement. At most two (constraints, phase) entries are allowed per child per
/// invocation; finalizing a provisional measurement counts as the correction.
/// Any failed request aborts the pass, even if the parent ignores its error.
pub(crate) trait ChildMeasurer {
    fn len(&self) -> usize;
    fn parent_data(&self, index: usize) -> Result<ParentData, LayoutError>;
    fn measure(&mut self, index: usize, constraints: BoxConstraints) -> Result<Size, LayoutError>;

    /// Whether this invocation is a natural-size probe. Ordinary child requests
    /// inherit the phase, including through legacy single-child adapters.
    fn is_provisional(&self) -> bool {
        false
    }

    /// Probe a child's natural layout while deferring nested stretch corrections.
    /// A finalizing parent must subsequently measure it normally before commit.
    /// Simple size-only test doubles can use the default implementation.
    fn measure_provisional(
        &mut self,
        index: usize,
        constraints: BoxConstraints,
    ) -> Result<Size, LayoutError> {
        self.measure(index, constraints)
    }
}

/// A parent's completed result. Each immediate child must be measured and placed
/// exactly once; placement indices do not change the tree's paint order.
pub(crate) struct ParentLayout {
    pub size: Size,
    pub placements: Vec<(usize, Point)>,
    pub diagnostics: Vec<LayoutDiagnostic>,
}

pub(crate) fn valid_size(size: Size) -> bool {
    size.width.is_finite() && size.height.is_finite() && size.width >= 0.0 && size.height >= 0.0
}

pub(crate) fn valid_point(point: Point) -> bool {
    point.x.is_finite() && point.y.is_finite()
}

pub(crate) fn valid_diagnostic(diagnostic: &LayoutDiagnostic) -> bool {
    match *diagnostic {
        LayoutDiagnostic::Overflow { main, cross } => valid_size(Size::new(main, cross)),
        _ => true,
    }
}

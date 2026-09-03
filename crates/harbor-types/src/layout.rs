//! Domain layout and session models for multi-session tabs and split panes.

// ── Identifiers ─────────────────────────────────────────────────────────────

/// Stable identifier for a top-level terminal session (tab).
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SessionId(pub u64);

/// Stable identifier for an individual terminal pane tile within a session.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PaneId(pub u64);

// ── Layout Orientation Enums ────────────────────────────────────────────────

/// Axis along which a split container divides its available space.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum SplitDirection {
    /// Children are placed side-by-side (left and right).
    #[default]
    Horizontal,
    /// Children are placed stacked (top and bottom).
    Vertical,
}

/// Screen edge on which the session TabBar is rendered.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum TabBarPosition {
    /// Top horizontal tab strip.
    #[default]
    Top,
    /// Bottom horizontal tab strip.
    Bottom,
    /// Left vertical sidebar tab strip.
    Left,
    /// Right vertical sidebar tab strip.
    Right,
}

impl TabBarPosition {
    /// Returns `true` if the tab bar is oriented horizontally along the top or bottom edge.
    pub fn is_horizontal(self) -> bool {
        matches!(self, Self::Top | Self::Bottom)
    }

    /// Returns `true` if the tab bar is oriented vertically along the left or right edge.
    pub fn is_vertical(self) -> bool {
        matches!(self, Self::Left | Self::Right)
    }
}

// ── Pane Layout Tree ────────────────────────────────────────────────────────

/// Recursive binary layout tree describing the nested tile splits in a session.
#[derive(Clone, Debug, PartialEq)]
pub enum PaneLayoutNode {
    /// A single terminal pane leaf.
    Leaf(PaneId),
    /// A binary split dividing space between two sub-trees with a fractional ratio (0.0..1.0).
    Split {
        direction: SplitDirection,
        ratio: f32,
        first: Box<PaneLayoutNode>,
        second: Box<PaneLayoutNode>,
    },
}

impl PaneLayoutNode {
    /// Creates a leaf node containing a single pane.
    pub fn leaf(id: PaneId) -> Self {
        Self::Leaf(id)
    }

    /// Creates a split node with two children.
    pub fn split(
        direction: SplitDirection,
        ratio: f32,
        first: PaneLayoutNode,
        second: PaneLayoutNode,
    ) -> Self {
        Self::Split {
            direction,
            ratio: ratio.clamp(0.05, 0.95),
            first: Box::new(first),
            second: Box::new(second),
        }
    }

    /// Recursively searches for the leaf node with `target` pane ID and splits it
    /// into two equal halves (ratio 0.5), placing the original pane in `first` and
    /// the newly created `new_id` in `second`.
    pub fn split_leaf(&mut self, target: PaneId, new_id: PaneId, dir: SplitDirection) -> bool {
        match self {
            Self::Leaf(id) if *id == target => {
                *self = Self::Split {
                    direction: dir,
                    ratio: 0.5,
                    first: Box::new(Self::Leaf(target)),
                    second: Box::new(Self::Leaf(new_id)),
                };
                true
            }
            Self::Split { first, second, .. } => {
                first.split_leaf(target, new_id, dir) || second.split_leaf(target, new_id, dir)
            }
            _ => false,
        }
    }

    /// Recursively removes `target` leaf from the tree.
    ///
    /// When `target` is found, its sibling node is promoted to replace the parent `Split` node.
    /// Returns `Some(target)` if the leaf was removed, or `None` if the target wasn't found
    /// or was the sole root leaf (which cannot be removed via sibling promotion).
    pub fn remove_leaf(&mut self, target: PaneId) -> Option<PaneId> {
        let (found, promoted) = match self {
            Self::Leaf(_) => return None,
            Self::Split { first, second, .. } => {
                if let Self::Leaf(id) = first.as_ref() {
                    if *id == target {
                        (true, Some(*second.clone()))
                    } else {
                        (false, None)
                    }
                } else {
                    (false, None)
                }
            }
        };

        if found {
            if let Some(replacement) = promoted {
                *self = replacement;
                return Some(target);
            }
        }

        let (found_sec, promoted_sec) = match self {
            Self::Split { first, second, .. } => {
                if let Self::Leaf(id) = second.as_ref() {
                    if *id == target {
                        (true, Some(*first.clone()))
                    } else {
                        (false, None)
                    }
                } else {
                    (false, None)
                }
            }
            _ => (false, None),
        };

        if found_sec {
            if let Some(replacement) = promoted_sec {
                *self = replacement;
                return Some(target);
            }
        }

        match self {
            Self::Split { first, second, .. } => first
                .remove_leaf(target)
                .or_else(|| second.remove_leaf(target)),
            _ => None,
        }
    }

    /// Checks if `target` pane ID exists in this layout tree.
    pub fn find_leaf(&self, target: PaneId) -> bool {
        match self {
            Self::Leaf(id) => *id == target,
            Self::Split { first, second, .. } => {
                first.find_leaf(target) || second.find_leaf(target)
            }
        }
    }

    /// Collects all `PaneId`s in visual depth-first order.
    pub fn collect_panes(&self) -> Vec<PaneId> {
        let mut panes = Vec::new();
        self.collect_panes_into(&mut panes);
        panes
    }

    fn collect_panes_into(&self, panes: &mut Vec<PaneId>) {
        match self {
            Self::Leaf(id) => panes.push(*id),
            Self::Split { first, second, .. } => {
                first.collect_panes_into(panes);
                second.collect_panes_into(panes);
            }
        }
    }

    /// Returns the first leaf `PaneId` in the tree.
    pub fn first_leaf(&self) -> PaneId {
        match self {
            Self::Leaf(id) => *id,
            Self::Split { first, .. } => first.first_leaf(),
        }
    }

    /// Finds the adjacent pane ID relative to `target` (forward or backward).
    pub fn find_adjacent_leaf(&self, target: PaneId, forward: bool) -> Option<PaneId> {
        let panes = self.collect_panes();
        let idx = panes.iter().position(|&p| p == target)?;
        if forward {
            if idx + 1 < panes.len() {
                Some(panes[idx + 1])
            } else {
                Some(panes[0])
            }
        } else if idx > 0 {
            Some(panes[idx - 1])
        } else {
            Some(panes[panes.len() - 1])
        }
    }
}

// ── Terminal Session Model ──────────────────────────────────────────────────

/// State of an active terminal workspace session (tab).
#[derive(Clone, Debug, PartialEq)]
pub struct TerminalSession {
    /// Unique session identifier.
    pub id: SessionId,
    /// Human-readable title displayed on the tab.
    pub title: String,
    /// Root pane layout tree.
    pub layout: PaneLayoutNode,
    /// Active/focused pane ID.
    pub active_pane: PaneId,
}

impl TerminalSession {
    /// Creates a new terminal session with a single pane.
    pub fn new(id: SessionId, pane_id: PaneId, title: impl Into<String>) -> Self {
        Self {
            id,
            title: title.into(),
            layout: PaneLayoutNode::leaf(pane_id),
            active_pane: pane_id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_leaf_creation_and_traversal() {
        let node = PaneLayoutNode::leaf(PaneId(1));
        assert_eq!(node.collect_panes(), vec![PaneId(1)]);
        assert!(node.find_leaf(PaneId(1)));
        assert!(!node.find_leaf(PaneId(2)));
        assert_eq!(node.first_leaf(), PaneId(1));
    }

    #[test]
    fn split_leaf_insertion() {
        let mut node = PaneLayoutNode::leaf(PaneId(1));
        assert!(node.split_leaf(PaneId(1), PaneId(2), SplitDirection::Horizontal));
        assert_eq!(node.collect_panes(), vec![PaneId(1), PaneId(2)]);

        // Nested split on pane 2
        assert!(node.split_leaf(PaneId(2), PaneId(3), SplitDirection::Vertical));
        assert_eq!(node.collect_panes(), vec![PaneId(1), PaneId(2), PaneId(3)]);
    }

    #[test]
    fn sibling_promotion_on_leaf_removal() {
        let mut node = PaneLayoutNode::leaf(PaneId(1));
        node.split_leaf(PaneId(1), PaneId(2), SplitDirection::Horizontal);
        node.split_leaf(PaneId(2), PaneId(3), SplitDirection::Vertical);

        // Remove pane 3 -> pane 2 promoted to take full second slot of root split
        let removed = node.remove_leaf(PaneId(3));
        assert_eq!(removed, Some(PaneId(3)));
        assert_eq!(node.collect_panes(), vec![PaneId(1), PaneId(2)]);

        // Remove pane 2 -> pane 1 promoted to become the root node
        let removed2 = node.remove_leaf(PaneId(2));
        assert_eq!(removed2, Some(PaneId(2)));
        assert_eq!(node, PaneLayoutNode::leaf(PaneId(1)));
    }

    #[test]
    fn adjacent_leaf_navigation() {
        let mut node = PaneLayoutNode::leaf(PaneId(1));
        node.split_leaf(PaneId(1), PaneId(2), SplitDirection::Horizontal);
        node.split_leaf(PaneId(2), PaneId(3), SplitDirection::Horizontal);

        assert_eq!(node.find_adjacent_leaf(PaneId(1), true), Some(PaneId(2)));
        assert_eq!(node.find_adjacent_leaf(PaneId(2), true), Some(PaneId(3)));
        assert_eq!(node.find_adjacent_leaf(PaneId(3), true), Some(PaneId(1)));

        assert_eq!(node.find_adjacent_leaf(PaneId(1), false), Some(PaneId(3)));
        assert_eq!(node.find_adjacent_leaf(PaneId(2), false), Some(PaneId(1)));
    }
    #[test]
    fn sibling_promotion_on_first_leaf_removal() {
        let mut node = PaneLayoutNode::leaf(PaneId(1));
        node.split_leaf(PaneId(1), PaneId(2), SplitDirection::Horizontal);
        node.split_leaf(PaneId(2), PaneId(3), SplitDirection::Vertical);

        let removed = node.remove_leaf(PaneId(1));
        assert_eq!(removed, Some(PaneId(1)));
        assert_eq!(node.collect_panes(), vec![PaneId(2), PaneId(3)]);
        assert_eq!(node.first_leaf(), PaneId(2));
        assert!(!node.find_leaf(PaneId(1)));
    }

    #[test]
    fn removing_missing_or_root_leaf_is_noop() {
        let mut node = PaneLayoutNode::leaf(PaneId(1));
        assert_eq!(node.remove_leaf(PaneId(1)), None);
        assert_eq!(node, PaneLayoutNode::leaf(PaneId(1)));

        node.split_leaf(PaneId(1), PaneId(2), SplitDirection::Horizontal);
        let snapshot = node.clone();
        assert_eq!(node.remove_leaf(PaneId(99)), None);
        assert_eq!(node, snapshot);
    }
}

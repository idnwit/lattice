//! Split tree: layouts are data (specs/03-sessions.md, Model). Gutters are
//! 1px of `line` showing through.

use nsh_core::prelude::*;

use crate::style::HAIRLINE;
use crate::tokens::color;

/// How a split lays out its children.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    /// Side by side: split right (`⌘D`).
    Row,
    /// Stacked: split down (`⌘⇧D`).
    Column,
}

/// A split tree with leaves of type `L` (a pane, an id).
#[derive(Debug, Clone, PartialEq)]
pub enum SplitNode<L> {
    /// One pane.
    Leaf(L),
    /// Children along `axis`, each with a flex weight.
    Split {
        /// Layout direction.
        axis: Axis,
        /// Children and their weights.
        children: Vec<(f32, SplitNode<L>)>,
    },
}

impl<L> SplitNode<L> {
    /// The leaves, in reading order.
    pub fn leaves(&self) -> Vec<&L> {
        match self {
            Self::Leaf(l) => vec![l],
            Self::Split { children, .. } => children.iter().flat_map(|(_, c)| c.leaves()).collect(),
        }
    }

    /// Splits the first leaf `at` matches along `axis`: it keeps its place
    /// and `new` goes after it with an equal weight. False when none matches.
    pub fn split(&mut self, at: &dyn Fn(&L) -> bool, axis: Axis, new: L) -> bool {
        let mut new = Some(new);
        self.split_inner(at, axis, &mut new)
    }

    fn split_inner(&mut self, at: &dyn Fn(&L) -> bool, axis: Axis, new: &mut Option<L>) -> bool {
        match self {
            Self::Leaf(l) if at(l) => {
                let Some(added) = new.take() else {
                    return false;
                };
                let old = std::mem::replace(
                    self,
                    Self::Split {
                        axis,
                        children: Vec::new(),
                    },
                );
                if let Self::Split { children, .. } = self {
                    children.push((1.0, old));
                    children.push((1.0, Self::Leaf(added)));
                }
                true
            }
            Self::Leaf(_) => false,
            Self::Split { children, .. } => children
                .iter_mut()
                .any(|(_, c)| c.split_inner(at, axis, new)),
        }
    }

    /// Removes the first leaf `at` matches; a split left with one child
    /// becomes that child. `None` when none matches or it is the only leaf
    /// (a tab's last pane closes the tab instead).
    pub fn remove(&mut self, at: &dyn Fn(&L) -> bool) -> Option<L> {
        let Self::Split { children, .. } = self else {
            return None;
        };
        let hit = children
            .iter()
            .position(|(_, c)| matches!(c, Self::Leaf(l) if at(l)));
        let removed = match hit {
            Some(i) => match children.remove(i) {
                (_, Self::Leaf(l)) => Some(l),
                (_, Self::Split { .. }) => None,
            },
            None => children.iter_mut().find_map(|(_, c)| c.remove(at)),
        };
        if children.len() == 1
            && let Some((_, only)) = children.pop()
        {
            *self = only;
        }
        removed
    }
}

/// Draws `node`; `leaf` draws each leaf.
pub fn split_view<L>(node: &SplitNode<L>, leaf: &dyn Fn(&L) -> View) -> View {
    match node {
        SplitNode::Leaf(l) => leaf(l),
        SplitNode::Split { axis, children } => view()
            .flex_1()
            .when(*axis == Axis::Row, Styled::flex_row)
            .gap(px(HAIRLINE))
            .bg(color::LINE)
            .children(children.iter().map(|(weight, child)| {
                view()
                    .flex_grow(*weight)
                    .flex_shrink(1.0)
                    .basis(px(0.0))
                    .min_w(px(0.0))
                    .min_h(px(0.0))
                    .child(split_view(child, leaf))
            })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(n: &SplitNode<u32>) -> Vec<u32> {
        n.leaves().into_iter().copied().collect()
    }

    #[test]
    fn split_puts_the_new_leaf_after_the_target() {
        let mut tree = SplitNode::Leaf(1);
        assert!(tree.split(&|l| *l == 1, Axis::Row, 2));
        assert!(tree.split(&|l| *l == 1, Axis::Column, 3));
        assert_eq!(ids(&tree), [1, 3, 2]);
        assert!(!tree.split(&|l| *l == 9, Axis::Row, 4), "no target");
    }

    #[test]
    fn remove_collapses_a_split_left_with_one_child() {
        let mut tree = SplitNode::Leaf(1);
        tree.split(&|l| *l == 1, Axis::Row, 2);
        tree.split(&|l| *l == 2, Axis::Column, 3);
        assert_eq!(tree.remove(&|l| *l == 3), Some(3));
        assert_eq!(tree.remove(&|l| *l == 1), Some(1));
        assert_eq!(tree, SplitNode::Leaf(2));
        assert_eq!(tree.remove(&|l| *l == 2), None, "last leaf stays");
    }
}

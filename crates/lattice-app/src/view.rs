//! View trees: what a viewer emits and the deck draws (05-deck.md, View-tree protocol; D43).
//!
//! A viewer is pure: it holds a [`Replica`] of the instance state, renders it as a flat
//! [`ViewTree`], and turns a [`UiEvent`] into app actions. It never draws pixels: the deck's
//! interpreter maps each [`Widget`] onto `lattice-deck-ui` components.
//!
//! Rules:
//! - A tree is a flat list of nodes. A node names its parent by index; a parent comes before
//!   its children, and children draw in list order. No parent: a root, drawn in the pane.
//! - Keys are chosen by the viewer, unique within one tree. Events name the node by key.
//! - Vocabulary v0. Enums only grow at the end (postcard tags are declaration order).
//! - Actions are fire and forget in v0: the outcome comes back as state, never to the viewer.
//!   TBD: tickets for viewers (an error toast for a refused action).
//!
//! The same [`ViewerCore`] runs natively (tests) and inside a WASM guest
//! (`lattice-app-guest`, `export_viewer!`); the deck holds it as a [`DynViewer`].

use std::cell::RefCell;

use lattice_state::Synced;
use serde::{Deserialize, Serialize};

use crate::call::{Caller, RawCallError, RawTicket};
use crate::replica::{Replica, ReplicaEvent};

/// Index of a node in its [`ViewTree`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeRef(u32);

impl NodeRef {
    /// The index.
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// One node: a viewer-chosen key, its parent, and a widget with its props.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Node {
    /// Unique in the tree; what [`UiEvent::key`] names.
    pub key: u32,
    /// Parent node; `None` for a root.
    pub parent: Option<NodeRef>,
    /// What to draw.
    pub widget: Widget,
}

/// Space between children, as a token step (`space/1`..`space/3`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Gap {
    /// None.
    None,
    /// `space/1`.
    Small,
    /// `space/2`.
    Medium,
    /// `space/3`.
    Large,
}

/// A color role. Every role maps to a token on the deck.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Tone {
    /// Row output text (`fg/term`).
    Text,
    /// Primary text (`fg`).
    Bright,
    /// Secondary text (`fg/muted`).
    Muted,
    /// Meta text (`fg/dim`).
    Dim,
    /// Prompt and cursor (`accent`).
    Accent,
    /// Needs-input mark (`waiting`).
    Waiting,
    /// One of the 16 ANSI colors; other indexes fall back to `fg/term`.
    Ansi(u8),
    /// A color the app's data names (a 256-color or true-color run), not a role.
    Rgb(u8, u8, u8),
    /// The pane background (`bg/0`): the text color of an inverse run with the default
    /// background.
    Base,
}

/// A run of text in one style.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Run {
    /// The text.
    pub text: String,
    /// Its color.
    pub tone: Tone,
    /// Its background; `None`: the pane's.
    pub bg: Option<Tone>,
    /// Bold weight.
    pub bold: bool,
    /// Faint.
    pub dim: bool,
    /// Slanted.
    pub italic: bool,
    /// Underlined.
    pub underline: bool,
}

impl Run {
    /// `text` in `tone`, on the pane's background, no emphasis.
    pub fn plain(text: impl Into<String>, tone: Tone) -> Self {
        Self {
            text: text.into(),
            tone,
            bg: None,
            bold: false,
            dim: false,
            italic: false,
            underline: false,
        }
    }
}

/// One monospace row: runs left to right, then an optional block cursor.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Line {
    /// The runs.
    pub runs: Vec<Run>,
    /// The row ends with the block cursor.
    pub cursor: bool,
}

/// Widget vocabulary v0, with its props.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Widget {
    /// Children stacked top to bottom. `grow`: takes the free height of its parent.
    Column {
        /// Space between children.
        gap: Gap,
        /// Fills the free space along its parent's axis.
        grow: bool,
    },
    /// Children side by side.
    Row {
        /// Space between children.
        gap: Gap,
        /// Fills the free space along its parent's axis.
        grow: bool,
    },
    /// One line of body text.
    Text {
        /// The text.
        text: String,
        /// Its color.
        tone: Tone,
    },
    /// Monospace rows, newest at the bottom; fills the free space.
    Lines {
        /// The rows.
        lines: Vec<Line>,
    },
    /// Rows like [`Widget::Lines`] that take the keyboard: each key is an
    /// [`EventKind::Key`]; its size in cells is an [`EventKind::Resize`].
    Terminal {
        /// The rows.
        lines: Vec<Line>,
    },
    /// A button: [`EventKind::Press`].
    Button {
        /// Its label.
        label: String,
    },
    /// A one-line text field: Enter is [`EventKind::Submit`] with the text; the field clears.
    Input {
        /// Shown while empty.
        placeholder: String,
    },
}

/// A flat view tree (module doc, Rules).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ViewTree {
    nodes: Vec<Node>,
}

/// Why a tree is refused by the deck.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ViewError {
    /// A node names a parent that does not come before it.
    #[error("parent after child")]
    BadParent,
    /// Two nodes share a key.
    #[error("duplicate key")]
    DuplicateKey,
}

impl ViewError {
    /// Stable code.
    pub fn code(&self) -> &'static str {
        match self {
            Self::BadParent => "view.bad_parent",
            Self::DuplicateKey => "view.duplicate_key",
        }
    }
}

impl ViewTree {
    /// An empty tree: the pane shows nothing.
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends a node under `parent` (a root for `None`) and returns its ref.
    pub fn push(&mut self, parent: Option<NodeRef>, key: u32, widget: Widget) -> NodeRef {
        let at = NodeRef(u32::try_from(self.nodes.len()).unwrap_or(u32::MAX));
        self.nodes.push(Node {
            key,
            parent,
            widget,
        });
        at
    }

    /// Every node, parents first.
    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }

    /// The children of each node, by index, in draw order; and the roots.
    ///
    /// # Errors
    ///
    /// [`ViewError`] when a parent comes after its child or a key repeats.
    pub fn children(&self) -> Result<(Vec<Vec<usize>>, Vec<usize>), ViewError> {
        let mut kids = vec![Vec::new(); self.nodes.len()];
        let mut roots = Vec::new();
        let mut keys = std::collections::BTreeSet::new();
        for (i, node) in self.nodes.iter().enumerate() {
            if !keys.insert(node.key) {
                return Err(ViewError::DuplicateKey);
            }
            match node.parent {
                None => roots.push(i),
                Some(p) if p.index() < i => {
                    kids.get_mut(p.index()).ok_or(ViewError::BadParent)?.push(i);
                }
                Some(_) => return Err(ViewError::BadParent),
            }
        }
        Ok((kids, roots))
    }
}

/// A key press, as the deck heard it (`Enter`, `ArrowUp`, `a`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyPress {
    /// Key name: a character, or a named key (`Enter`, `Backspace`, `ArrowLeft`).
    pub key: String,
    /// Control held.
    pub ctrl: bool,
    /// Option / Alt held.
    pub alt: bool,
    /// Shift held.
    pub shift: bool,
    /// Command / Super held.
    pub meta: bool,
}

/// What happened on a node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventKind {
    /// A [`Widget::Button`] was pressed.
    Press,
    /// A [`Widget::Input`] was submitted with this text.
    Submit(String),
    /// A key went down in a [`Widget::Terminal`].
    Key(KeyPress),
    /// A [`Widget::Terminal`] now fits `cols` by `rows` cells (its first layout, or a change).
    Resize {
        /// Columns.
        cols: u16,
        /// Rows.
        rows: u16,
    },
}

/// One UI event for the viewer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiEvent {
    /// The node's key.
    pub key: u32,
    /// What happened.
    pub kind: EventKind,
}

/// Collects the actions a viewer sends while it handles one event: a [`Caller`] for the
/// app's generated client (`EchoApiClient::new(actions)`).
#[derive(Debug, Default)]
pub struct Actions(RefCell<Vec<Vec<u8>>>);

impl Actions {
    /// The encoded actions, in call order.
    pub(crate) fn into_vec(self) -> Vec<Vec<u8>> {
        self.0.into_inner()
    }
}

impl Caller for &Actions {
    /// Queues `action`. The ticket settles `Unknown`: v0 viewers never hear outcomes.
    fn call(&self, action: Vec<u8>) -> RawTicket {
        self.0.borrow_mut().push(action);
        RawTicket::ready(Err(RawCallError::Unknown))
    }
}

/// The deck side of an app (06-apps.md, Runner and viewer). Pure: no I/O, no clock.
pub trait Viewer: Default {
    /// The instance state, as the runner commits it.
    type State: Synced + Default;

    /// Draws `state`.
    fn render(&self, state: &Self::State) -> ViewTree;

    /// Handles one event; actions for the runner go through `actions`.
    fn handle(&mut self, state: &Self::State, event: &UiEvent, actions: &Actions);
}

/// Object-safe view of a viewer, so the deck holds viewers of different apps.
pub trait DynViewer {
    /// Replaces the state with a full state at `version`.
    fn load(&mut self, version: u64, bytes: &[u8]) -> ReplicaEvent;
    /// Applies the delta at `version`; [`ReplicaEvent::Resync`] when it does not follow.
    fn apply(&mut self, version: u64, bytes: &[u8]) -> ReplicaEvent;
    /// The state version held; `None` before the first full state.
    fn version(&self) -> Option<u64>;
    /// Draws the state.
    fn render(&mut self) -> ViewTree;
    /// Handles one event; returns the encoded actions to send, in order.
    fn handle(&mut self, event: &UiEvent) -> Vec<Vec<u8>>;
    /// True once the viewer can take no more calls (a WASM guest trapped).
    fn is_dead(&self) -> bool {
        false
    }
}

/// A [`Viewer`] with its replica: what a guest exports and what tests drive natively.
#[derive(Default)]
pub struct ViewerCore<V: Viewer> {
    viewer: V,
    replica: Replica<V::State>,
}

impl<V: Viewer> DynViewer for ViewerCore<V> {
    fn load(&mut self, version: u64, bytes: &[u8]) -> ReplicaEvent {
        self.replica.on_full_state(version, bytes)
    }

    fn apply(&mut self, version: u64, bytes: &[u8]) -> ReplicaEvent {
        self.replica.on_delta(version, bytes)
    }

    fn version(&self) -> Option<u64> {
        self.replica.version()
    }

    fn render(&mut self) -> ViewTree {
        self.viewer.render(self.replica.state())
    }

    fn handle(&mut self, event: &UiEvent) -> Vec<Vec<u8>> {
        let actions = Actions::default();
        self.viewer.handle(self.replica.state(), event, &actions);
        actions.into_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lattice_state::{Draft, Value, encode};

    #[derive(Default)]
    struct Counter;

    impl Viewer for Counter {
        type State = Value<u32>;

        fn render(&self, state: &Value<u32>) -> ViewTree {
            let mut tree = ViewTree::new();
            let col = tree.push(
                None,
                1,
                Widget::Column {
                    gap: Gap::Small,
                    grow: true,
                },
            );
            let text = format!("{}", state.get());
            tree.push(
                Some(col),
                2,
                Widget::Text {
                    text,
                    tone: Tone::Bright,
                },
            );
            tree.push(Some(col), 3, Widget::Button { label: "+".into() });
            tree
        }

        fn handle(&mut self, state: &Value<u32>, event: &UiEvent, actions: &Actions) {
            if event.key == 3 && event.kind == EventKind::Press {
                drop(actions.call(vec![u8::try_from(*state.get()).unwrap_or(0)]));
            }
        }
    }

    #[test]
    fn a_core_renders_its_replica_and_turns_events_into_actions() {
        let mut core = ViewerCore::<Counter>::default();
        assert_eq!(
            core.load(0, &encode(&Value::new(4u32)).unwrap()),
            ReplicaEvent::Changed
        );
        let mut runner = Value::new(4u32);
        runner.set(7);
        let delta = encode(&Draft(&runner)).unwrap();
        assert_eq!(core.apply(1, &delta), ReplicaEvent::Changed);
        assert_eq!(core.apply(1, &delta), ReplicaEvent::Resync);
        let tree = core.render();
        let text = tree.nodes().get(1).map(|n| n.widget.clone());
        assert_eq!(
            text,
            Some(Widget::Text {
                text: "7".into(),
                tone: Tone::Bright
            })
        );
        let press = UiEvent {
            key: 3,
            kind: EventKind::Press,
        };
        assert_eq!(core.handle(&press), vec![vec![7]]);
        let other = UiEvent {
            key: 2,
            kind: EventKind::Press,
        };
        assert!(core.handle(&other).is_empty());
    }

    #[test]
    fn children_follow_list_order_and_bad_trees_are_refused() {
        let mut tree = ViewTree::new();
        let root = tree.push(
            None,
            1,
            Widget::Column {
                gap: Gap::None,
                grow: true,
            },
        );
        tree.push(Some(root), 2, Widget::Button { label: "a".into() });
        tree.push(Some(root), 3, Widget::Button { label: "b".into() });
        let (kids, roots) = tree.children().unwrap();
        assert_eq!((kids.first().cloned(), roots), (Some(vec![1, 2]), vec![0]));

        let mut dup = tree.clone();
        dup.push(
            None,
            2,
            Widget::Text {
                text: String::new(),
                tone: Tone::Text,
            },
        );
        assert_eq!(dup.children().unwrap_err().code(), "view.duplicate_key");
        let mut ahead = ViewTree::new();
        ahead.push(Some(NodeRef(1)), 1, Widget::Button { label: "x".into() });
        assert_eq!(ahead.children().unwrap_err().code(), "view.bad_parent");
    }
}

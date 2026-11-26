//! Deck side: the echo viewer (feature `viewer`; 05-deck.md, View-tree protocol).
//!
//! The lines, oldest first, then a field and `Clear`. Enter sends the field's text
//! (`send`); `Clear` empties the lines (`clear`). A refused send shows nothing in v0
//! (TBD: an error note once viewers hear outcomes).

use lattice_app::view::{
    Actions, EventKind, Gap, Line, Run, Tone, UiEvent, ViewTree, Viewer, Widget,
};

use crate::api::{EchoApiClient, Message};
use crate::model::EchoState;

/// Keys of the nodes.
const ROOT: u32 = 1;
const LINES: u32 = 2;
const BAR: u32 = 3;
const FIELD: u32 = 4;
const CLEAR: u32 = 5;

/// The echo viewer. Holds no UI state: the field keeps its own draft.
#[derive(Debug, Default)]
pub struct EchoViewer;

impl Viewer for EchoViewer {
    type State = EchoState;

    fn render(&self, state: &EchoState) -> ViewTree {
        let mut tree = ViewTree::new();
        let root = tree.push(
            None,
            ROOT,
            Widget::Column {
                gap: Gap::None,
                grow: true,
            },
        );
        tree.push(
            Some(root),
            LINES,
            Widget::Lines {
                lines: lines(state),
            },
        );
        let bar = tree.push(
            Some(root),
            BAR,
            Widget::Row {
                gap: Gap::Medium,
                grow: false,
            },
        );
        let placeholder = "Send a line".to_owned();
        tree.push(Some(bar), FIELD, Widget::Input { placeholder });
        let label = "Clear".to_owned();
        tree.push(Some(bar), CLEAR, Widget::Button { label });
        tree
    }

    fn handle(&mut self, _state: &EchoState, event: &UiEvent, actions: &Actions) {
        let client = EchoApiClient::new(actions);
        match (event.key, &event.kind) {
            (FIELD, EventKind::Submit(text)) => drop(client.send(Message::new(text.clone()))),
            (CLEAR, EventKind::Press) => drop(client.clear(())),
            _ => {}
        }
    }
}

/// `   1  hello`: the sequence number dim, the text as row output. A hint when empty.
fn lines(state: &EchoState) -> Vec<Line> {
    let rows: Vec<Line> = state
        .lines()
        .iter()
        .map(|l| {
            let l = l.get();
            let seq = Run::plain(format!("{:>4}  ", l.seq()), Tone::Dim);
            let text = Run::plain(l.text(), Tone::Text);
            Line {
                runs: vec![seq, text],
                cursor: false,
            }
        })
        .collect();
    if rows.is_empty() {
        let hint = Run::plain("No lines yet. Type one below and press Enter.", Tone::Dim);
        return vec![Line {
            runs: vec![hint],
            cursor: false,
        }];
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use lattice_app::view::{DynViewer, ViewerCore};

    #[test]
    fn an_empty_state_shows_the_hint_field_and_clear() {
        let mut core = ViewerCore::<EchoViewer>::default();
        let tree = core.render();
        let keys: Vec<u32> = tree.nodes().iter().map(|n| n.key).collect();
        assert_eq!(keys, [ROOT, LINES, BAR, FIELD, CLEAR]);
        assert!(tree.children().is_ok());
        let Some(Widget::Lines { lines }) = tree.nodes().get(1).map(|n| n.widget.clone()) else {
            panic!("lines node");
        };
        assert_eq!(lines.len(), 1, "the hint");
    }

    #[test]
    fn submit_sends_and_clear_clears() {
        let mut core = ViewerCore::<EchoViewer>::default();
        let submit = UiEvent {
            key: FIELD,
            kind: EventKind::Submit("hi".to_owned()),
        };
        let clear = UiEvent {
            key: CLEAR,
            kind: EventKind::Press,
        };
        let sent = core.handle(&submit);
        let cleared = core.handle(&clear);
        assert_eq!((sent.len(), cleared.len()), (1, 1));
        assert_ne!(sent, cleared, "two different actions");
        let stray = UiEvent {
            key: LINES,
            kind: EventKind::Press,
        };
        assert!(core.handle(&stray).is_empty());
    }
}

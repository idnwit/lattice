//! The view-tree interpreter: a viewer's [`ViewTree`] drawn with `lattice-deck-ui`
//! (05-deck.md, View-tree protocol; D14, D42).
//!
//! Rules:
//! - Elements are made again only when the tree's shape changes: its keys, parents, widget
//!   kinds, and container props. Other props (text, rows, labels) update in place, so a
//!   terminal keeps the keyboard and a field keeps its draft while state streams in.
//! - Every color comes from a token ([`tone`]).
//! - A tree the deck refuses ([`ViewTree::children`]) shows its error code instead.

use std::rc::Rc;

use lattice_app::view::{self, EventKind, Gap, KeyPress, UiEvent, ViewTree, Widget};
use lattice_deck_ui::components::{
    ButtonKind, Density, Line, Run, Tone, button, cell_grid, cell_probe, prompt_input, styled_lines,
};
use lattice_deck_ui::style::{HAIRLINE, meta};
use lattice_deck_ui::tokens::{color, radius, row, space};
use nsh_core::prelude::*;

/// What a node is, as far as making elements goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Column(Gap, bool),
    Row(Gap, bool),
    Text,
    Lines,
    Terminal,
    Button,
    Input,
}

/// The shape of a tree: per node its key, parent index, and kind. `Err` holds the refusal code.
type Shape = Result<Vec<(u32, Option<usize>, Kind)>, &'static str>;

fn shape(tree: &ViewTree) -> Shape {
    tree.children().map_err(|e| e.code())?;
    Ok(tree
        .nodes()
        .iter()
        .map(|n| (n.key, n.parent.map(view::NodeRef::index), kind(&n.widget)))
        .collect())
}

fn kind(w: &Widget) -> Kind {
    match w {
        Widget::Column { gap, grow } => Kind::Column(*gap, *grow),
        Widget::Row { gap, grow } => Kind::Row(*gap, *grow),
        Widget::Text { .. } => Kind::Text,
        Widget::Lines { .. } => Kind::Lines,
        Widget::Terminal { .. } => Kind::Terminal,
        Widget::Button { .. } => Kind::Button,
        Widget::Input { .. } => Kind::Input,
    }
}

/// Draws the tree `tree` gives; events go to `on_event`. `focused`: a terminal in it takes
/// the keyboard when made.
pub(crate) fn tree_view(
    tree: impl Fn() -> ViewTree + 'static,
    focused: bool,
    on_event: Rc<dyn Fn(UiEvent)>,
) -> View {
    let tree = computed(tree);
    let shape = computed(move || tree.with(shape));
    view()
        .flex_1()
        .min_w(px(0.0))
        .min_h(px(0.0))
        .child(dynamic(move || match shape.get() {
            Err(code) => meta(code).text_color(color::FG_DIM).into_element(),
            Ok(nodes) => {
                let b = Build {
                    nodes: Rc::new(nodes),
                    tree,
                    focused,
                    on_event: on_event.clone(),
                };
                let roots: Vec<View> = (0..b.nodes.len())
                    .filter(|&i| b.parent(i).is_none())
                    .map(|i| b.node(i))
                    .collect();
                view()
                    .flex_1()
                    .min_h(px(0.0))
                    .children(roots)
                    .into_element()
            }
        }))
}

struct Build {
    nodes: Rc<Vec<(u32, Option<usize>, Kind)>>,
    tree: Memo<ViewTree>,
    focused: bool,
    on_event: Rc<dyn Fn(UiEvent)>,
}

impl Build {
    fn parent(&self, i: usize) -> Option<usize> {
        self.nodes.get(i).and_then(|n| n.1)
    }

    fn node(&self, i: usize) -> View {
        let Some(&(key, _, kind)) = self.nodes.get(i) else {
            return view();
        };
        let tree = self.tree;
        let widget = move || {
            tree.with(|t| {
                t.nodes()
                    .iter()
                    .find(|n| n.key == key)
                    .map(|n| n.widget.clone())
            })
        };
        let send = self.on_event.clone();
        let event = move |kind: EventKind| send(UiEvent { key, kind });
        match kind {
            Kind::Column(gap, grow) | Kind::Row(gap, grow) => {
                let kids: Vec<View> = (0..self.nodes.len())
                    .filter(|&c| self.parent(c) == Some(i))
                    .map(|c| self.node(c))
                    .collect();
                let row = matches!(kind, Kind::Row(..));
                view()
                    .gap(px(gap_px(gap)))
                    .when(row, |v| v.flex_row().items_center())
                    .when(grow, |v| v.flex_1().min_w(px(0.0)).min_h(px(0.0)))
                    .children(kids)
            }
            Kind::Text => view().child(dynamic(move || match widget() {
                Some(Widget::Text { text, tone: t }) => lattice_deck_ui::style::body(
                    text,
                    lattice_deck_ui::components::tone_color(tone(t)),
                )
                .into_element(),
                _ => view().into_element(),
            })),
            Kind::Lines => view().flex_1().min_h(px(0.0)).child(dynamic(move || {
                styled_lines(&lines(widget()), Density::Regular)
            })),
            Kind::Terminal => terminal(self.focused, widget, event),
            Kind::Button => view().child(dynamic(move || {
                let label = match widget() {
                    Some(Widget::Button { label }) => label,
                    _ => String::new(),
                };
                let event = event.clone();
                button(ButtonKind::Secondary, &label, None, row::ROW_SM)
                    .on_press(move |_: &PressEvent| event(EventKind::Press))
            })),
            Kind::Input => {
                let name = match widget() {
                    Some(Widget::Input { placeholder }) => placeholder,
                    _ => String::new(),
                };
                field(&name, event)
            }
        }
    }
}

/// A terminal: rows that take the keyboard (click to focus). Keys go out as events, and so
/// does its size in cells: measured once laid out, and again when the window resizes.
fn terminal(
    focused: bool,
    widget: impl Fn() -> Option<Widget> + 'static,
    event: impl Fn(EventKind) + 'static,
) -> View {
    let target = element_ref();
    let probe = element_ref();
    let event: Rc<dyn Fn(EventKind)> = Rc::new(event);
    let sent = stored(None::<(u16, u16)>);
    let resize = event.clone();
    let measure = Rc::new(move || {
        let resize = resize.clone();
        spawn(async move {
            let (Some(pane), Some(cell)) = (target.measure().await, probe.measure().await) else {
                return;
            };
            let (pane, cell) = (pane.frame.size, cell.frame.size);
            let grid = cell_grid(pane.width.0, pane.height.0, cell.width.0, cell.height.0);
            if sent.with(|s| s.replace(grid)) != Some(grid) {
                resize(EventKind::Resize {
                    cols: grid.0,
                    rows: grid.1,
                });
            }
        });
    });
    // The view is laid out (and `focus()` works) from the next frame on.
    let first = measure.clone();
    request_animation_frame(move |_| {
        if focused {
            target.focus();
        }
        first();
    });
    // A window resize moves every pane: measure again once it is laid out.
    let window_size = computed(|| {
        let w = window();
        (w.width.0.to_bits(), w.height.0.to_bits())
    });
    effect(move || {
        window_size.get();
        let again = measure.clone();
        request_animation_frame(move |_| again());
    });
    let keys = event;
    view()
        .flex_1()
        .min_h(px(0.0))
        .relative()
        .element_ref(&target)
        .focusable(true)
        .label("Terminal".to_owned())
        .capture_keys(KeyCapture::All)
        .on_press(move |_: &PressEvent| target.focus())
        .on_key_down(move |ev: &KeyEvent| keys(EventKind::Key(key_press(ev))))
        .child(cell_probe(Density::Regular).element_ref(&probe))
        .child(dynamic(move || {
            styled_lines(&lines(widget()), Density::Regular)
        }))
}

/// A one-line field in a bordered box. Enter submits and clears it; focus stays.
fn field(name: &str, event: impl Fn(EventKind) + 'static) -> View {
    let draft = signal(String::new());
    view()
        .flex_1()
        .min_w(px(0.0))
        .h(px(row::ROW_MD))
        .px(px(space::SP_3))
        .justify_center()
        .rounded(px(radius::R_3))
        .border(px(HAIRLINE))
        .border_color(color::LINE_STRONG)
        .bg(color::BG_0)
        .child(prompt_input(name, draft, Density::Regular, move |text| {
            draft.set(String::new());
            event(EventKind::Submit(text));
        }))
}

fn key_press(ev: &KeyEvent) -> KeyPress {
    KeyPress {
        key: ev.key.to_string(),
        ctrl: ev.modifiers.ctrl,
        alt: ev.modifiers.alt,
        shift: ev.modifiers.shift,
        meta: ev.modifiers.cmd,
    }
}

fn gap_px(gap: Gap) -> f32 {
    match gap {
        Gap::None => 0.0,
        Gap::Small => space::SP_1,
        Gap::Medium => space::SP_2,
        Gap::Large => space::SP_3,
    }
}

/// The rows of a `Lines` or `Terminal` widget, as `styled_lines` takes them.
fn lines(widget: Option<Widget>) -> Vec<Line> {
    let rows = match widget {
        Some(Widget::Lines { lines } | Widget::Terminal { lines }) => lines,
        _ => Vec::new(),
    };
    rows.into_iter()
        .map(|l| Line {
            runs: l.runs.into_iter().map(run).collect(),
            cursor: l.cursor,
        })
        .collect()
}

/// A view run as a `lattice-deck-ui` run.
fn run(r: view::Run) -> Run {
    Run {
        text: r.text,
        tone: tone(r.tone),
        bg: r.bg.map(tone),
        bold: r.bold,
        dim: r.dim,
        italic: r.italic,
        underline: r.underline,
    }
}

/// A view tone as a `lattice-deck-ui` tone (roles map to tokens; `Rgb` is the app's data).
fn tone(t: view::Tone) -> Tone {
    match t {
        view::Tone::Text => Tone::Text,
        view::Tone::Bright => Tone::Bright,
        view::Tone::Muted => Tone::Muted,
        view::Tone::Dim => Tone::Dim,
        view::Tone::Accent => Tone::Accent,
        view::Tone::Waiting => Tone::Waiting,
        view::Tone::Ansi(n) => Tone::Ansi(n),
        view::Tone::Rgb(r, g, b) => Tone::Rgb(r, g, b),
        view::Tone::Base => Tone::Base,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lattice_app::view::Line as ViewLine;

    fn tree(text: &str) -> ViewTree {
        let mut t = ViewTree::new();
        let col = t.push(
            None,
            1,
            Widget::Column {
                gap: Gap::Small,
                grow: true,
            },
        );
        let lines = vec![ViewLine {
            runs: vec![view::Run::plain(text, view::Tone::Ansi(2))],
            cursor: true,
        }];
        t.push(Some(col), 2, Widget::Lines { lines });
        t
    }

    #[test]
    fn new_text_keeps_the_shape_and_a_new_node_changes_it() {
        assert_eq!(shape(&tree("a")), shape(&tree("b")));
        let mut more = tree("a");
        more.push(None, 3, Widget::Button { label: "x".into() });
        assert_ne!(shape(&tree("a")), shape(&more));
        let mut bad = tree("a");
        bad.push(None, 2, Widget::Button { label: "x".into() });
        assert_eq!(shape(&bad), Err("view.duplicate_key"));
    }

    #[test]
    fn rows_keep_their_runs_tones_and_cursor() {
        let got = lines(tree("hi").nodes().get(1).map(|n| n.widget.clone()));
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].runs, [Run::new("hi", Tone::Ansi(2))]);
        assert!(got[0].cursor);
        assert!(lines(None).is_empty());
    }
}

//! Deck side: the terminal viewer (feature `viewer`; 05-deck.md, View-tree protocol).
//!
//! Renders the screen rows as one [`Widget::Terminal`]: the 16 ANSI colors become token
//! roles, 256-color and true-color runs keep their color ([`Tone::Rgb`]), backgrounds,
//! bold, dim, italic, and underline carry over, inverse swaps the colors, and the cursor row
//! ends with the block cursor. Keys become `input` bytes (xterm
//! conventions). The pane's size in cells becomes `resize`, unless the state already has it.
//! TBD: scrollback view.

use lattice_app::view::{
    Actions, EventKind, KeyPress, Line, Run as ViewRun, Tone, UiEvent, ViewTree, Viewer, Widget,
};

use crate::api::{Input, Resize, TerminalApiClient};
use crate::model::{Attrs, Color, Row, TermState};

/// Key of the terminal node.
const SCREEN: u32 = 1;

/// The terminal viewer. Holds no UI state.
#[derive(Debug, Default)]
pub struct TermViewer;

impl Viewer for TermViewer {
    type State = TermState;

    fn render(&self, state: &TermState) -> ViewTree {
        let mut tree = ViewTree::new();
        tree.push(
            None,
            SCREEN,
            Widget::Terminal {
                lines: lines(state),
            },
        );
        tree
    }

    fn handle(&mut self, state: &TermState, event: &UiEvent, actions: &Actions) {
        if event.key != SCREEN {
            return;
        }
        let client = TerminalApiClient::new(actions);
        match &event.kind {
            EventKind::Key(key) => {
                if let Some(bytes) = key_bytes(key) {
                    // Fire and forget: the echo comes back as state.
                    drop(client.input(Input::new(bytes)));
                }
            }
            EventKind::Resize { cols, rows } => {
                let size = state.size();
                if (size.cols(), size.rows()) != (*cols, *rows) {
                    drop(client.resize(Resize::new(*cols, *rows)));
                }
            }
            EventKind::Press | EventKind::Submit(_) => {}
        }
    }
}

/// Screen rows as view lines; the cursor row ends with the block cursor.
fn lines(state: &TermState) -> Vec<Line> {
    let cursor = state.cursor();
    let (_, cursor_row) = cursor.position();
    state
        .screen()
        .iter()
        .enumerate()
        .map(|(i, row)| line(row.get(), cursor.visible() && usize::from(cursor_row) == i))
        .collect()
}

fn line(row: &Row, cursor: bool) -> Line {
    let runs = row.runs().iter().map(run).collect();
    Line { runs, cursor }
}

/// One styled run: colors as tones, inverse resolved (fg and bg swap; the defaults become
/// terminal text and the pane background).
fn run(r: &crate::model::Run) -> ViewRun {
    let attrs = r.attrs();
    let (mut fg, mut bg) = (tone(r.fg()), bg_tone(r.bg()));
    if attrs.contains(Attrs::INVERSE) {
        (fg, bg) = (bg.unwrap_or(Tone::Base), Some(fg));
    }
    ViewRun {
        text: r.text().to_owned(),
        tone: fg,
        bg,
        bold: attrs.contains(Attrs::BOLD),
        dim: attrs.contains(Attrs::DIM),
        italic: attrs.contains(Attrs::ITALIC),
        underline: attrs.contains(Attrs::UNDERLINE),
    }
}

/// A foreground color as a tone: the 16 ANSI slots as token roles, the rest as data.
fn tone(color: Color) -> Tone {
    match color {
        Color::Default => Tone::Text,
        Color::Indexed(i) if i < 16 => Tone::Ansi(i),
        Color::Indexed(i) => {
            let (r, g, b) = xterm_rgb(i);
            Tone::Rgb(r, g, b)
        }
        Color::Rgb(r, g, b) => Tone::Rgb(r, g, b),
    }
}

/// A background color as a tone; the default is the pane's (`None`).
fn bg_tone(color: Color) -> Option<Tone> {
    (color != Color::Default).then(|| tone(color))
}

/// The xterm 256-color palette above 15: the 6×6×6 cube, then 24 grays.
fn xterm_rgb(i: u8) -> (u8, u8, u8) {
    const STEPS: [u8; 6] = [0, 95, 135, 175, 215, 255];
    let step = |n: u8| STEPS.get(usize::from(n)).copied().unwrap_or(255);
    match i {
        16..=231 => {
            let n = i - 16;
            (step(n / 36), step((n / 6) % 6), step(n % 6))
        }
        232..=255 => {
            let v = 8 + (i - 232) * 10;
            (v, v, v)
        }
        _ => (0, 0, 0),
    }
}

/// Bytes a key sends to the shell (xterm conventions). `None` for keys that send nothing.
pub fn key_bytes(key: &KeyPress) -> Option<Vec<u8>> {
    let named: Option<&[u8]> = match key.key.as_str() {
        "Enter" => Some(b"\r"),
        "Backspace" => Some(b"\x7f"),
        "Tab" => Some(b"\t"),
        "Escape" => Some(b"\x1b"),
        "ArrowUp" => Some(b"\x1b[A"),
        "ArrowDown" => Some(b"\x1b[B"),
        "ArrowRight" => Some(b"\x1b[C"),
        "ArrowLeft" => Some(b"\x1b[D"),
        "Home" => Some(b"\x1b[H"),
        "End" => Some(b"\x1b[F"),
        "Delete" => Some(b"\x1b[3~"),
        _ => None,
    };
    if let Some(b) = named {
        return Some(b.to_vec());
    }
    let mut chars = key.key.chars();
    let (Some(c), None) = (chars.next(), chars.next()) else {
        return None;
    };
    if key.meta {
        return None;
    }
    if key.ctrl && c.is_ascii_alphabetic() {
        let lower = u8::try_from(c.to_ascii_lowercase()).ok()?;
        return Some(vec![lower & 0x1f]);
    }
    let mut out = Vec::new();
    if key.alt {
        out.push(0x1b);
    }
    let mut buf = [0u8; 4];
    out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lattice_app::view::{DynViewer, ViewerCore};

    fn press(key: &str) -> KeyPress {
        KeyPress {
            key: key.to_owned(),
            ctrl: false,
            alt: false,
            shift: false,
            meta: false,
        }
    }

    #[test]
    fn keys_follow_xterm_conventions() {
        let cases: [(KeyPress, Option<&[u8]>); 7] = [
            (press("a"), Some(b"a")),
            (press("Enter"), Some(b"\r")),
            (press("ArrowUp"), Some(b"\x1b[A")),
            (
                KeyPress {
                    ctrl: true,
                    ..press("C")
                },
                Some(b"\x03"),
            ),
            (
                KeyPress {
                    alt: true,
                    ..press("b")
                },
                Some(b"\x1bb"),
            ),
            (
                KeyPress {
                    meta: true,
                    ..press("c")
                },
                None,
            ),
            (press("F13"), None),
        ];
        for (key, want) in cases {
            assert_eq!(key_bytes(&key).as_deref(), want, "case={}", key.key);
        }
    }

    #[test]
    fn an_empty_state_renders_one_terminal_and_a_key_sends_input() {
        let mut core = ViewerCore::<TermViewer>::default();
        let tree = core.render();
        assert_eq!(
            tree.nodes().first().map(|n| n.widget.clone()),
            Some(Widget::Terminal { lines: Vec::new() })
        );
        let key = UiEvent {
            key: SCREEN,
            kind: EventKind::Key(press("x")),
        };
        assert_eq!(core.handle(&key).len(), 1, "one input action");
        let other = UiEvent {
            key: SCREEN,
            kind: EventKind::Press,
        };
        assert!(core.handle(&other).is_empty());
    }

    #[test]
    fn a_new_size_sends_resize_and_the_size_held_sends_nothing() {
        let mut core = ViewerCore::<TermViewer>::default();
        let resize = |cols, rows| UiEvent {
            key: SCREEN,
            kind: EventKind::Resize { cols, rows },
        };
        assert_eq!(core.handle(&resize(120, 40)).len(), 1);
        // A new state is 0 by 0 until the runner sets its size.
        assert!(core.handle(&resize(0, 0)).is_empty());
    }

    #[test]
    fn colors_map_to_ansi_slots_or_their_own_color() {
        assert_eq!(tone(Color::Default), Tone::Text);
        assert_eq!(tone(Color::Indexed(1)), Tone::Ansi(1));
        assert_eq!(tone(Color::Indexed(208)), Tone::Rgb(255, 135, 0));
        assert_eq!(tone(Color::Indexed(16)), Tone::Rgb(0, 0, 0));
        assert_eq!(tone(Color::Indexed(232)), Tone::Rgb(8, 8, 8));
        assert_eq!(tone(Color::Indexed(255)), Tone::Rgb(238, 238, 238));
        assert_eq!(tone(Color::Rgb(1, 2, 3)), Tone::Rgb(1, 2, 3));
        assert_eq!(bg_tone(Color::Default), None);
    }

    #[test]
    fn inverse_swaps_colors_and_attrs_carry_over() {
        let r = crate::model::Run::new(
            "x".to_owned(),
            Color::Indexed(1),
            Color::Default,
            Attrs::INVERSE.with(Attrs::BOLD).with(Attrs::UNDERLINE),
        );
        let v = run(&r);
        assert_eq!((v.tone, v.bg), (Tone::Base, Some(Tone::Ansi(1))));
        assert!(v.bold && v.underline && !v.italic && !v.dim);
    }
}

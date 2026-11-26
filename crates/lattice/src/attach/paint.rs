//! The instance screen as bytes for the local terminal (specs/03-sessions.md, Attach rules
//! 6, 12–16). Sans-IO: frames in, escape sequences out.

use std::fmt::Write;

use lattice_app_terminal::{Attrs, Color, Cursor, Run};

/// Sent once on attach: alternate screen, line wrap off (rule 6).
pub(super) const ENTER: &[u8] = b"\x1b[?1049h\x1b[?7l";
/// Sent on every exit path: plain style, line wrap on, cursor shown, main screen (rule 12).
pub(super) const LEAVE: &[u8] = b"\x1b[0m\x1b[?7h\x1b[?25h\x1b[?1049l";

/// Draws frames, redrawing only what changed since the last one.
#[derive(Debug)]
pub(super) struct Painter {
    /// Local rows.
    height: u16,
    /// Rows on the local screen; `None` until the first frame or after a resize.
    drawn: Option<Vec<Vec<Run>>>,
    /// Cursor on the local screen.
    cursor: Option<Cursor>,
}

impl Painter {
    /// A painter for a local terminal `height` rows tall.
    pub(super) fn new(height: u16) -> Self {
        Self {
            height,
            drawn: None,
            cursor: None,
        }
    }

    /// The local terminal is now `height` rows tall; the next frame redraws all rows.
    pub(super) fn resize(&mut self, height: u16) {
        self.height = height;
        self.drawn = None;
    }

    /// Bytes that turn the local screen into `rows` with `cursor`. Empty when nothing changed.
    pub(super) fn paint(&mut self, rows: &[&[Run]], cursor: Cursor) -> Vec<u8> {
        let shown = rows.get(..usize::from(self.height)).unwrap_or(rows);
        let full = self.drawn.is_none();
        let drawn = self.drawn.take().unwrap_or_default();
        let changed: Vec<usize> = (0..shown.len())
            .filter(|&i| full || drawn.get(i).map(Vec::as_slice) != shown.get(i).copied())
            .collect();
        self.drawn = Some(shown.iter().map(|r| r.to_vec()).collect());
        if changed.is_empty() && self.cursor == Some(cursor) {
            return Vec::new();
        }
        self.cursor = Some(cursor);
        let mut out = String::from("\x1b[?25l");
        if full {
            out.push_str("\x1b[H\x1b[2J");
        }
        for i in changed {
            draw_row(&mut out, i, shown.get(i).copied().unwrap_or_default());
        }
        let (col, row) = cursor.position();
        push(
            &mut out,
            format_args!("\x1b[{};{}H", u32::from(row) + 1, u32::from(col) + 1),
        );
        if cursor.visible() {
            out.push_str("\x1b[?25h");
        }
        out.into_bytes()
    }
}

/// Row `i` (zero-based): its runs, then erase to the end of the line.
fn draw_row(out: &mut String, i: usize, runs: &[Run]) {
    push(out, format_args!("\x1b[{};1H", i.saturating_add(1)));
    for run in runs {
        push(out, format_args!("\x1b[{}m", sgr(run)));
        // Rule 15: cell text never reaches the local terminal as a control sequence.
        out.extend(
            run.text()
                .chars()
                .map(|c| if c.is_control() { ' ' } else { c }),
        );
    }
    out.push_str("\x1b[0m\x1b[K");
}

/// The SGR parameters of a run, after a reset (`0`).
fn sgr(run: &Run) -> String {
    let mut p = String::from("0");
    let attrs = [
        (Attrs::BOLD, 1),
        (Attrs::DIM, 2),
        (Attrs::ITALIC, 3),
        (Attrs::UNDERLINE, 4),
        (Attrs::INVERSE, 7),
    ];
    for (attr, code) in attrs {
        if run.attrs().contains(attr) {
            push(&mut p, format_args!(";{code}"));
        }
    }
    color(&mut p, run.fg(), 30, 90, 38);
    color(&mut p, run.bg(), 40, 100, 48);
    p
}

/// Appends one color (rule 14): `base` + 0–7, `bright` + 0–7 for 8–15, else `ext;5;n` or `ext;2;r;g;b`.
fn color(p: &mut String, c: Color, base: u8, bright: u8, ext: u8) {
    match c {
        Color::Default => {}
        Color::Indexed(n @ 0..8) => push(p, format_args!(";{}", u16::from(base) + u16::from(n))),
        Color::Indexed(n @ 8..16) => {
            push(p, format_args!(";{}", u16::from(bright) + u16::from(n) - 8))
        }
        Color::Indexed(n) => push(p, format_args!(";{ext};5;{n}")),
        Color::Rgb(r, g, b) => push(p, format_args!(";{ext};2;{r};{g};{b}")),
    }
}

/// Appends formatted text; writing to a `String` cannot fail.
fn push(out: &mut String, args: std::fmt::Arguments<'_>) {
    // `fmt::Write for String` never returns an error.
    out.write_fmt(args).unwrap_or_default();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(text: &str) -> Run {
        Run::new(text.to_owned(), Color::Default, Color::Default, Attrs::NONE)
    }

    fn text(bytes: &[u8]) -> String {
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    #[test]
    fn first_frame_clears_and_draws_every_row_then_the_cursor() {
        let mut p = Painter::new(24);
        let (a, b) = ([run("$ ls")], [run("x")]);
        let got = text(&p.paint(&[&a, &[], &b], Cursor::new(4, 0, true)));
        let want = "\x1b[?25l\x1b[H\x1b[2J\
                    \x1b[1;1H\x1b[0m$ ls\x1b[0m\x1b[K\
                    \x1b[2;1H\x1b[0m\x1b[K\
                    \x1b[3;1H\x1b[0mx\x1b[0m\x1b[K\
                    \x1b[1;5H\x1b[?25h";
        assert_eq!(got, want);
    }

    #[test]
    fn unchanged_frame_writes_nothing() {
        let mut p = Painter::new(24);
        let a = [run("$ ")];
        let cursor = Cursor::new(2, 0, true);
        assert!(!p.paint(&[&a], cursor).is_empty());
        assert!(p.paint(&[&a], cursor).is_empty());
    }

    #[test]
    fn only_changed_rows_are_redrawn() {
        let mut p = Painter::new(24);
        let (a, b, c) = ([run("one")], [run("two")], [run("TWO")]);
        p.paint(&[&a, &b], Cursor::new(0, 0, true));
        let got = text(&p.paint(&[&a, &c], Cursor::new(0, 0, true)));
        assert!(got.contains("\x1b[2;1H\x1b[0mTWO"), "{got:?}");
        assert!(!got.contains("one"), "row 1 redrawn: {got:?}");
        assert!(!got.contains("\x1b[2J"), "cleared: {got:?}");
    }

    #[test]
    fn cursor_move_alone_redraws_no_row() {
        let mut p = Painter::new(24);
        let a = [run("abc")];
        p.paint(&[&a], Cursor::new(0, 0, true));
        let got = text(&p.paint(&[&a], Cursor::new(2, 0, true)));
        assert_eq!(got, "\x1b[?25l\x1b[1;3H\x1b[?25h");
    }

    #[test]
    fn local_resize_redraws_all_rows() {
        let mut p = Painter::new(24);
        let a = [run("abc")];
        p.paint(&[&a], Cursor::new(0, 0, true));
        p.resize(30);
        let got = text(&p.paint(&[&a], Cursor::new(0, 0, true)));
        assert!(got.contains("\x1b[2J\x1b[1;1H\x1b[0mabc"), "{got:?}");
    }

    #[test]
    fn rows_past_the_local_height_are_not_drawn() {
        let mut p = Painter::new(1);
        let (a, b) = ([run("top")], [run("below")]);
        let got = text(&p.paint(&[&a, &b], Cursor::new(0, 0, true)));
        assert!(got.contains("top"), "{got:?}");
        assert!(!got.contains("below"), "{got:?}");
    }

    #[test]
    fn hidden_cursor_stays_hidden() {
        let mut p = Painter::new(24);
        let got = text(&p.paint(&[], Cursor::new(0, 0, false)));
        assert!(got.ends_with("\x1b[1;1H"), "{got:?}");
        assert!(!got.contains("?25h"), "{got:?}");
    }

    #[test]
    fn control_characters_in_cells_are_drawn_as_spaces() {
        let mut p = Painter::new(24);
        let evil = [run("a\x1b]0;pwn\x07b\u{9b}c\x7f")];
        let got = text(&p.paint(&[&evil], Cursor::new(0, 0, true)));
        assert!(got.contains("a ]0;pwn b c "), "{got:?}");
        assert!(!got.contains('\x07') && !got.contains('\u{9b}') && !got.contains('\x7f'));
    }

    #[test]
    fn run_style_becomes_sgr_parameters() {
        let styled = |fg, bg, attrs| Run::new(String::new(), fg, bg, attrs);
        let d = Color::Default;
        let cases = [
            ("default", styled(d, d, Attrs::NONE), "0"),
            ("ansi fg", styled(Color::Indexed(1), d, Attrs::NONE), "0;31"),
            (
                "bright fg",
                styled(Color::Indexed(9), d, Attrs::NONE),
                "0;91",
            ),
            (
                "cube fg",
                styled(Color::Indexed(200), d, Attrs::NONE),
                "0;38;5;200",
            ),
            (
                "rgb fg",
                styled(Color::Rgb(1, 2, 3), d, Attrs::NONE),
                "0;38;2;1;2;3",
            ),
            ("ansi bg", styled(d, Color::Indexed(4), Attrs::NONE), "0;44"),
            (
                "bright bg",
                styled(d, Color::Indexed(12), Attrs::NONE),
                "0;104",
            ),
            (
                "cube bg",
                styled(d, Color::Indexed(16), Attrs::NONE),
                "0;48;5;16",
            ),
            (
                "rgb bg",
                styled(d, Color::Rgb(9, 8, 7), Attrs::NONE),
                "0;48;2;9;8;7",
            ),
            ("bold", styled(d, d, Attrs::BOLD), "0;1"),
            (
                "all attrs",
                styled(
                    d,
                    d,
                    Attrs::BOLD
                        .with(Attrs::DIM)
                        .with(Attrs::ITALIC)
                        .with(Attrs::UNDERLINE)
                        .with(Attrs::INVERSE),
                ),
                "0;1;2;3;4;7",
            ),
        ];
        for (name, run, want) in cases {
            assert_eq!(sgr(&run), want, "case={name}");
        }
    }
}

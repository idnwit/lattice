//! Styled lines: rows of colored text runs with an optional block cursor.
//!
//! For any monospace row output: a terminal screen, logs, diffs. Rows are
//! drawn as styled text runs rather than NSH's `Terminal` element: that
//! element paints ANSI colors from a fixed palette of its own, while ours
//! must come from `tokens.json`.

use nsh_core::prelude::*;

use crate::style::typed;
use crate::tokens::{ANSI, color, space, state, typo};

/// Compact row size: 12/18 (canvas: secondary panes and mobile).
const COMPACT: (f32, f32) = (12.0, 18.0);

/// A color role for a run of text. Every role maps to a token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    /// Body text of row output (`fg/term`).
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
    /// One of the 16 ANSI colors (`ansi[i]`); other indexes fall back to `fg/term`.
    Ansi(u8),
    /// A color the row's data names (a 256-color or true-color terminal run), not a role.
    Rgb(u8, u8, u8),
    /// The rows' background (`bg/0`): text of an inverse run.
    Base,
}

/// The color of a tone.
pub fn tone_color(tone: Tone) -> Rgba {
    match tone {
        Tone::Text => color::FG_TERM,
        Tone::Bright => color::FG,
        Tone::Muted => color::FG_MUTED,
        Tone::Dim => color::FG_DIM,
        Tone::Accent => color::ACCENT,
        Tone::Waiting => state::WAITING,
        Tone::Ansi(n) => ANSI.get(usize::from(n)).copied().unwrap_or(color::FG_TERM),
        Tone::Rgb(r, g, b) => rgb(u32::from_be_bytes([0, r, g, b])),
        Tone::Base => color::BG_0,
    }
}

/// A run of text in one style.
#[derive(Debug, Clone, PartialEq)]
pub struct Run {
    /// The text.
    pub text: String,
    /// Its color role.
    pub tone: Tone,
    /// Its background; `None`: the rows'.
    pub bg: Option<Tone>,
    /// Bold weight.
    pub bold: bool,
    /// Faint: half alpha.
    pub dim: bool,
    /// Slanted.
    pub italic: bool,
    /// Underlined.
    pub underline: bool,
}

impl Run {
    /// A run of `text` in `tone`, on the rows' background, no emphasis.
    pub fn new(text: impl Into<String>, tone: Tone) -> Self {
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

/// One row: runs left to right, then an optional block cursor.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Line {
    /// The runs.
    pub runs: Vec<Run>,
    /// The row ends with the block cursor (`accent`).
    pub cursor: bool,
}

impl Line {
    /// A row of `runs` without a cursor.
    pub fn new(runs: Vec<Run>) -> Self {
        Self {
            runs,
            cursor: false,
        }
    }
}

/// Row text density.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Density {
    /// `mono/term` (13/20): the focused pane.
    Regular,
    /// 12/18: secondary panes and mobile.
    Compact,
}

impl Density {
    /// Text size and line height, in points.
    fn metrics(self) -> (f32, f32) {
        match self {
            Self::Regular => (typo::MONO_TERM.size, typo::MONO_TERM.line),
            Self::Compact => COMPACT,
        }
    }
}

/// Padding of [`styled_lines`] inside its box: (horizontal, vertical), each side.
const PAD: (f32, f32) = (space::SP_3, space::SP_2);

/// Characters in [`cell_probe`]; many, so rounding stays small.
const CELL_SAMPLE: usize = 32;

/// An invisible row of 32 cells drawn exactly as [`styled_lines`] draws rows.
/// Measure it (`ElementRef::measure`) and pass its size to [`cell_grid`]. Put it inside a
/// `relative` box: it is `absolute` and takes no space.
pub fn cell_probe(density: Density) -> View {
    let sample = Line::new(vec![Run::new("M".repeat(CELL_SAMPLE), Tone::Text)]);
    view()
        .absolute()
        .top(px(0.0))
        .left(px(0.0))
        .opacity(0.0)
        .child(styled_line(&sample, density))
}

/// How many cells `(cols, rows)` fit in a [`styled_lines`] box `width` by `height`
/// points, given the measured size of its [`cell_probe`]: the box less its padding, over
/// one cell. Each side is at least 1.
pub fn cell_grid(width: f32, height: f32, probe_width: f32, probe_height: f32) -> (u16, u16) {
    grid(
        width,
        height,
        probe_width / CELL_SAMPLE as f32,
        probe_height,
    )
}

/// [`cell_grid`] with one cell's width and height.
fn grid(width: f32, height: f32, cell: f32, line: f32) -> (u16, u16) {
    let fit = |len: f32, pad: f32, step: f32| {
        if step <= 0.0 {
            return 1;
        }
        let n = ((len - 2.0 * pad) / step).floor();
        // `as` saturates: NaN and negatives give 0, huge values u16::MAX.
        (n as u16).max(1)
    };
    (fit(width, PAD.0, cell), fit(height, PAD.1, line))
}

/// `lines` on `bg/0`, clipped to the box. The newest (last) rows stay in
/// view; older ones go off the top.
///
/// Rows keep a monospace grid, as terminals draw them: a Private Use Area icon (from the
/// symbols fallback) takes one cell, a wide glyph (CJK, emoji) two, any other glyph one.
///
/// TBD: virtualize long outputs (only the visible rows are drawn now, but all are built).
pub fn styled_lines(lines: &[Line], density: Density) -> View {
    let cell = cell_width(density);
    view()
        .flex_1()
        .min_h(px(0.0))
        .overflow_hidden()
        .justify_end()
        .px(px(PAD.0))
        .py(px(PAD.1))
        .bg(color::BG_0)
        .children(lines.iter().map(|l| row(l, density, cell)))
}

/// One row of [`styled_lines`].
pub(crate) fn styled_line(line: &Line, density: Density) -> Text {
    row(line, density, cell_width(density))
}

/// Alpha of a dim run's color: half intensity, as terminals draw SGR 2.
const DIM_ALPHA: f32 = 0.5;
/// Weight of a bold run (the bundled JetBrains Mono Bold).
const BOLD_WEIGHT: u16 = 700;

fn row(line: &Line, density: Density, cell: f32) -> Text {
    let (size, height) = density.metrics();
    let base = typed(text(""), typo::MONO_TERM)
        .text_size(px(size))
        .line_height(px(height))
        .text_color(color::FG_TERM)
        .whitespace_nowrap();
    // An empty row still takes its height.
    let base = if line.runs.iter().all(|r| r.text.is_empty()) {
        base.child(" ")
    } else {
        base.children(
            line.runs
                .iter()
                .flat_map(|r| pieces(&r.text, size, cell).map(move |(t, fix)| span(r, t, fix))),
        )
    };
    base.when(line.cursor, |t| {
        t.child(text("█").text_color(color::ACCENT))
    })
}

/// One styled span, held to one cell by `fit` when it is a lone glyph.
fn span(r: &Run, t: String, fit: Option<Fit>) -> Text {
    let mut fg = tone_color(r.tone);
    if r.dim {
        fg.a *= DIM_ALPHA;
    }
    // A span of spaces draws no glyph, and so no background: full blocks in the
    // background's color fill its cells instead.
    let (t, fg) = match r.bg {
        Some(bg) if t.chars().all(|c| c == ' ') => ("█".repeat(t.chars().count()), tone_color(bg)),
        _ => (t, fg),
    };
    text(t)
        .text_color(fg)
        .when_some(r.bg, |t, bg| t.bg(tone_color(bg)))
        .when(r.bold, |t| t.font_weight(FontWeight(BOLD_WEIGHT)))
        .when(r.italic, Styled::italic)
        .when(r.underline, Styled::underline)
        .when_some(fit, |t, fit| {
            t.when_some(fit.size, |t, s| t.text_size(px(s)))
                .letter_spacing(px(fit.spacing))
        })
}

/// How a lone glyph is held to one cell: drawn at `size` (when it changes), then `spacing`
/// after it so its advance is exactly one cell.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Fit {
    size: Option<f32>,
    spacing: f32,
}

/// How wide a shrunk icon's advance stays, in cells. Icon fonts leave side bearings in
/// the advance, so ink of this advance about fills one cell, as iTerm draws icons.
/// TBD: fit the ink bounds once NSH measures them.
const ICON_CELLS: f32 = 1.3;

/// `text` split so each glyph off the grid is a piece of its own, with the [`Fit`] that holds
/// it to its cells, as terminals draw it; other pieces need none. ASCII is always on the
/// grid. Off the grid: a Private Use Area icon (one cell, drawn a little larger, see
/// [`ICON_CELLS`]), a wide glyph (an advance over 1.5 cells: CJK and emoji, two cells; the
/// runner already dropped the cell after it), and any other glyph whose advance is not one
/// cell (one cell). A zero-width glyph (a combining mark) stays with its base.
fn pieces(text: &str, size: f32, cell: f32) -> impl Iterator<Item = (String, Option<Fit>)> {
    let mut out: Vec<(String, Option<Fit>)> = Vec::new();
    let mut plain = String::new();
    for c in text.chars() {
        match glyph_fit(c, size, cell) {
            Some(f) => {
                if !plain.is_empty() {
                    out.push((std::mem::take(&mut plain), None));
                }
                out.push((c.to_string(), Some(f)));
            }
            None => plain.push(c),
        }
    }
    if !plain.is_empty() {
        out.push((plain, None));
    }
    out.into_iter()
}

/// The fit of one glyph (see [`pieces`]); `None` when it is on the grid.
fn glyph_fit(c: char, size: f32, cell: f32) -> Option<Fit> {
    if c.is_ascii() {
        return None;
    }
    let advance = glyph_advance(c, size);
    if is_private_use(c) {
        fit(advance, size, cell, ICON_CELLS)
    } else if advance < ZERO_WIDTH * cell {
        None
    } else if advance > WIDE * cell {
        fit(advance, size, 2.0 * cell, 1.0)
    } else {
        fit(advance, size, cell, 1.0)
    }
}

/// An advance this close to its cells needs no fit.
const ADVANCE_SLACK: f32 = 0.01;
/// An advance over this many cells is a wide glyph.
const WIDE: f32 = 1.5;
/// An advance under this many cells is zero-width (a combining mark).
const ZERO_WIDTH: f32 = 0.25;

/// The fit that gives a glyph of `advance` an advance of exactly `cells` (points): drawn
/// at most `grow` times `cells` wide (shrunk when larger), then spaced back to `cells`.
fn fit(advance: f32, size: f32, cells: f32, grow: f32) -> Option<Fit> {
    let wide = grow * cells;
    if advance > wide + ADVANCE_SLACK {
        // An advance scales with the size: shrink to `wide`, then pull back.
        Some(Fit {
            size: Some(size * wide / advance),
            spacing: cells - wide,
        })
    } else if (advance - cells).abs() > ADVANCE_SLACK {
        Some(Fit {
            size: None,
            spacing: cells - advance,
        })
    } else {
        None
    }
}

/// Unicode Private Use Areas: where Nerd Font icons live.
fn is_private_use(c: char) -> bool {
    matches!(u32::from(c), 0xE000..=0xF8FF | 0xF_0000..=0xF_FFFD | 0x10_0000..=0x10_FFFD)
}

/// The mono font string of `size` for [`measure_text`].
fn mono_font(size: f32) -> String {
    format!("{size}px {}", typo::MONO_TERM.family)
}

/// Width of one monospace cell at `density`.
fn cell_width(density: Density) -> f32 {
    let (size, _) = density.metrics();
    measure_text(&mono_font(size), &"M".repeat(CELL_SAMPLE)).0 / CELL_SAMPLE as f32
}

/// Advance of one glyph at `size`, measured once per glyph and size.
fn glyph_advance(c: char, size: f32) -> f32 {
    thread_local! {
        static ADVANCES: std::cell::RefCell<std::collections::HashMap<(char, u32), f32>> =
            std::cell::RefCell::new(std::collections::HashMap::new());
    }
    ADVANCES.with(|m| {
        *m.borrow_mut()
            .entry((c, size.to_bits()))
            .or_insert_with(|| measure_text(&mono_font(size), &c.to_string()).0)
    })
}

/// A prompt row: `❯` and a real text field in the rows' font.
///
/// `name` is its accessible label; `draft` holds the text; `submit` hears
/// Enter with it. Focus stays in the field after Enter, as in a shell.
pub fn prompt_input(
    name: &str,
    draft: Signal<String>,
    density: Density,
    submit: impl Fn(String) + 'static,
) -> View {
    let (size, _) = density.metrics();
    let field = typed(nsh_core::element::input(), typo::MONO_TERM)
        .text_size(px(size))
        .flex_1()
        .min_w(px(0.0))
        .text_color(color::FG)
        .caret_color(color::ACCENT)
        .label(name.to_owned())
        .auto_correct(false)
        .auto_capitalize(AutoCapitalize::None)
        .submit_behavior(SubmitBehavior::Submit)
        .value(move || draft.get())
        .on_change(move |t: &str| draft.set(t.to_owned()))
        .on_submit(move |t: &str| submit(t.to_owned()));
    view()
        .flex_row()
        .items_center()
        .gap(px(space::SP_2))
        .child(
            typed(text("❯"), typo::MONO_TERM)
                .text_size(px(size))
                .text_color(color::ACCENT),
        )
        .child(field)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_use_glyphs_split_into_pieces_of_their_own() {
        assert!(is_private_use('\u{f179}') && !is_private_use('~'));
        let ascii: Vec<_> = pieces("ab ~", 13.0, 7.8).collect();
        assert_eq!(ascii, [("ab ~".to_owned(), None)]);
        let shrunk = fit(26.0, 13.0, 10.0, ICON_CELLS).unwrap();
        assert_eq!(
            shrunk.size,
            Some(13.0 * 13.0 / 26.0),
            "ICON_CELLS cells of advance"
        );
        assert!(
            (shrunk.spacing + 3.0).abs() < 1e-4,
            "pulled back to one cell"
        );
        let padded = fit(6.0, 13.0, 7.5, 1.0).unwrap();
        assert_eq!((padded.size, padded.spacing), (None, 1.5));
        assert_eq!(
            fit(9.0, 13.0, 8.0, ICON_CELLS).map(|f| f.size),
            Some(None),
            "a little wide: spacing only"
        );
        assert_eq!(fit(7.8, 13.0, 7.8, 1.0), None);
        let wide = fit(17.0, 13.0, 15.6, 1.0).unwrap();
        assert!(
            (wide.spacing).abs() < 1e-4,
            "a wide glyph shrinks to its two cells"
        );
    }

    #[test]
    fn tones_map_to_tokens_or_their_own_color() {
        assert_eq!(tone_color(Tone::Rgb(255, 0, 16)), rgb(0xFF0010));
        assert_eq!(tone_color(Tone::Base), color::BG_0);
        assert_eq!(tone_color(Tone::Ansi(99)), color::FG_TERM);
    }

    #[test]
    fn the_grid_is_the_box_less_padding_over_the_cell() {
        let (w, h) = (2.0 * PAD.0 + 80.0 * 8.0, 2.0 * PAD.1 + 24.0 * 20.0);
        assert_eq!(grid(w, h, 8.0, 20.0), (80, 24));
        assert_eq!(
            grid(w + 7.9, h + 19.0, 8.0, 20.0),
            (80, 24),
            "partial cells drop"
        );
        assert_eq!(grid(0.0, 0.0, 8.0, 20.0), (1, 1), "never zero");
        assert_eq!(grid(100.0, 100.0, 0.0, 20.0).0, 1, "no cell width");
    }
}

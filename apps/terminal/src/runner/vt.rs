//! The VT emulator: `vt100` behind a small, deterministic surface.
//!
//! Private runner memory. Nothing here reads a clock, the environment or the file
//! system, so the same bytes give the same grid in any chunking, on any target
//! (including `wasm32-unknown-unknown`).
//!
//! What `vt100` does not do, and how this module covers it:
//! - **Replies.** Device status reports (`CSI 5n`, `CSI 6n`) and primary device
//!   attributes (`CSI c`) are answered from the unhandled-CSI callback.
//! - **Synchronized updates** (`ESC[?2026h` … `ESC[?2026l`). `vt100` applies the bytes
//!   at once; this module only tracks the mode, so the runner can tell an `ESC[3J`
//!   sent inside an update.
//! - **Not supported:** concealed text (`SGR 8`) and strikeout (`SGR 9`) are ignored;
//!   bold and dim are one intensity (the last of `SGR 1` / `SGR 2` wins).

use std::num::NonZeroU16;

use vt100::{Callbacks, Parser, Screen};

use crate::model::{Attrs, Color, Cursor, Row, Run};

/// Emulator history between two drains. The runner feeds at most
/// [`SEGMENT_BYTES`] per drain, and one byte scrolls at most one line (except
/// `CSI n S`), so history never overflows between drains.
const HISTORY_LINES: usize = 10_000;

/// Most bytes fed between two history drains. Below [`HISTORY_LINES`].
pub(super) const SEGMENT_BYTES: usize = 4096;

/// `ESC[?2026`: synchronized update mode.
const SYNC_MODE: u16 = 2026;

/// Something the emulator told its listener.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Note {
    /// The window title changed; `None` resets it.
    Title(Option<String>),
    /// The emulator answers the program (cursor position report, device attributes).
    Reply(String),
}

/// Collects [`Note`]s and tracks what `vt100` leaves to its callbacks.
#[derive(Debug, Default)]
struct Notes {
    notes: Vec<Note>,
    /// Inside a synchronized update.
    syncing: bool,
}

impl Callbacks for Notes {
    fn set_window_title(&mut self, _: &mut Screen, title: &[u8]) {
        let title = (!title.is_empty()).then(|| String::from_utf8_lossy(title).into_owned());
        self.notes.push(Note::Title(title));
    }

    fn unhandled_csi(
        &mut self,
        screen: &mut Screen,
        i1: Option<u8>,
        _i2: Option<u8>,
        params: &[&[u16]],
        c: char,
    ) {
        let first = params.first().and_then(|p| p.first()).copied();
        match (i1, c) {
            (None, 'n') if first == Some(5) => self.reply("\x1b[0n".to_owned()),
            (None, 'n') if first == Some(6) => {
                let (row, col) = screen.cursor_position();
                let (row, col) = (u32::from(row) + 1, u32::from(col) + 1);
                self.reply(format!("\x1b[{row};{col}R"));
            }
            // Primary device attributes: a VT102, as alacritty answers.
            (None, 'c') if matches!(first, None | Some(0)) => self.reply("\x1b[?6c".to_owned()),
            (Some(b'?'), 'h' | 'l') if params.iter().any(|p| *p == [SYNC_MODE]) => {
                self.syncing = c == 'h';
            }
            // Clipboard, color and pixel-size requests need the deck: TBD. The rest is UI.
            _ => {}
        }
    }
}

impl Notes {
    fn reply(&mut self, text: String) {
        self.notes.push(Note::Reply(text));
    }
}

/// The emulator and its parser.
pub(super) struct Vt {
    parser: Parser<Notes>,
}

impl std::fmt::Debug for Vt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (rows, cols) = self.parser.screen().size();
        f.debug_struct("Vt")
            .field("cols", &cols)
            .field("rows", &rows)
            .finish_non_exhaustive()
    }
}

impl Vt {
    /// A blank emulator of `cols` by `rows`. Both are at least 1.
    pub(super) fn new(cols: u16, rows: u16) -> Self {
        let (rows, cols) = dims(cols, rows);
        Self {
            parser: Parser::new_with_callbacks(rows, cols, HISTORY_LINES, Notes::default()),
        }
    }

    /// Feeds output bytes.
    pub(super) fn feed(&mut self, bytes: &[u8]) {
        self.parser.process(bytes);
    }

    /// Resizes the grid. Lines pushed off the top land in history.
    pub(super) fn resize(&mut self, cols: u16, rows: u16) {
        let (rows, cols) = dims(cols, rows);
        self.parser.set_size(rows, cols);
    }

    /// Notes since the last call, in order.
    pub(super) fn take_notes(&mut self) -> Vec<Note> {
        std::mem::take(&mut self.parser.callbacks_mut().notes)
    }

    /// True while the alternate screen is up (`ESC[?1049h`).
    pub(super) fn in_alt_screen(&self) -> bool {
        self.parser.screen().alternate_screen()
    }

    /// True inside a synchronized update (`ESC[?2026h` until `ESC[?2026l`).
    pub(super) fn syncing(&self) -> bool {
        self.parser.callbacks().syncing
    }

    /// Visible rows.
    pub(super) fn rows(&self) -> usize {
        usize::from(self.parser.screen().size().0.get())
    }

    /// Visible row `index` (0 is the top), styled.
    pub(super) fn row(&self, index: usize) -> Row {
        to_row(
            self.parser.screen(),
            u16::try_from(index).unwrap_or(u16::MAX),
        )
    }

    /// Cursor on the visible screen.
    pub(super) fn cursor(&self) -> Cursor {
        let screen = self.parser.screen();
        let (row, col) = screen.cursor_position();
        Cursor::new(col, row, !screen.hide_cursor())
    }

    /// Takes the lines in history, oldest first, and empties it.
    ///
    /// The runner owns scrollback; the emulator only holds lines between two drains.
    /// So a resize never pulls old lines back onto the screen. While the alternate
    /// screen is up, main-screen history is out of reach: it is taken after it closes.
    pub(super) fn drain_history(&mut self) -> Vec<Row> {
        let screen = self.parser.screen_mut();
        if screen.alternate_screen() {
            return Vec::new();
        }
        let lines = screen.scrollback_len();
        let window = usize::from(screen.size().0.get());
        let mut rows = Vec::with_capacity(lines);
        // `vt100` shows history only through its viewport: scroll it back one
        // window at a time and read the window's rows.
        for start in (0..lines).step_by(window) {
            screen.set_scrollback(lines - start);
            let count = window.min(lines - start);
            rows.extend((0..count).map(|i| to_row(screen, u16::try_from(i).unwrap_or(u16::MAX))));
        }
        screen.set_scrollback(0);
        // Also forgets the lines it ever held, so growing the screen adds rows below.
        screen.clear_scrollback();
        rows
    }
}

/// `(rows, cols)`, each at least 1.
fn dims(cols: u16, rows: u16) -> (NonZeroU16, NonZeroU16) {
    let at_least_one = |n| NonZeroU16::new(n).unwrap_or(NonZeroU16::MIN);
    (at_least_one(rows), at_least_one(cols))
}

/// Visible row `index` of `screen` as runs. Trailing blanks are dropped; the right
/// halves of wide chars are skipped.
fn to_row(screen: &Screen, index: u16) -> Row {
    let cols = screen.size().1.get();
    let cells: Vec<&vt100::Cell> = (0..cols).map_while(|col| screen.cell(index, col)).collect();
    let end = cells
        .iter()
        .rposition(|c| !is_blank(c))
        .map_or(0, |i| i + 1);
    let mut runs: Vec<Run> = Vec::new();
    let mut text = String::new();
    let mut style = None;
    for cell in cells.iter().take(end) {
        if cell.is_wide_continuation() {
            continue;
        }
        let next = (color(cell.fgcolor()), color(cell.bgcolor()), attrs(cell));
        if style.is_some_and(|s| s != next) {
            runs.push(run(std::mem::take(&mut text), style));
        }
        style = Some(next);
        if cell.has_contents() {
            text.push_str(cell.contents());
        } else {
            text.push(' ');
        }
    }
    if !text.is_empty() {
        runs.push(run(text, style));
    }
    Row::new(runs)
}

fn run(text: String, style: Option<(Color, Color, Attrs)>) -> Run {
    let (fg, bg, attrs) = style.unwrap_or_default();
    Run::new(text, fg, bg, attrs)
}

/// A cell that shows nothing: a space on the default background, no visible line or swap.
fn is_blank(cell: &vt100::Cell) -> bool {
    matches!(cell.contents(), "" | " ")
        && color(cell.bgcolor()) == Color::Default
        && !cell.inverse()
        && !cell.underline()
}

fn attrs(cell: &vt100::Cell) -> Attrs {
    [
        (cell.bold(), Attrs::BOLD),
        (cell.italic(), Attrs::ITALIC),
        (cell.underline(), Attrs::UNDERLINE),
        (cell.inverse(), Attrs::INVERSE),
        (cell.dim(), Attrs::DIM),
    ]
    .into_iter()
    .filter(|(on, _)| *on)
    .fold(Attrs::NONE, |acc, (_, attr)| acc.with(attr))
}

/// Indexed colors 0–15 are palette entries (`SGR 30–37`, `90–97`); default is the theme's.
fn color(c: vt100::Color) -> Color {
    match c {
        vt100::Color::Default => Color::Default,
        vt100::Color::Idx(i) => Color::Indexed(i),
        vt100::Color::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}

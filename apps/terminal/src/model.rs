//! `TermState`: what decks hold and render (06-apps.md, State).

use lattice_state::{List, Synced, Value};
use serde::{Deserialize, Serialize};

/// One rendered row: styled runs of text, left to right.
///
/// Trailing blank cells (a space with default background and no visible attribute) are
/// dropped, so a blank row has no run. A wide character is one `char` in its run; its
/// spacer cell is not repeated.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Row {
    runs: Vec<Run>,
}

impl Row {
    /// A row of these runs.
    #[cfg_attr(
        not(feature = "runner"),
        allow(dead_code, reason = "built by the runner only")
    )]
    pub(crate) fn new(runs: Vec<Run>) -> Self {
        Self { runs }
    }

    /// The styled runs, left to right.
    pub fn runs(&self) -> &[Run] {
        &self.runs
    }

    /// The row text, without style.
    pub fn text(&self) -> String {
        self.runs.iter().map(Run::text).collect()
    }
}

/// A stretch of cells with one style.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Run {
    text: String,
    fg: Color,
    bg: Color,
    attrs: Attrs,
}

impl Run {
    /// A run of `text` in one style.
    pub fn new(text: String, fg: Color, bg: Color, attrs: Attrs) -> Self {
        Self {
            text,
            fg,
            bg,
            attrs,
        }
    }

    /// The text of the run.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Foreground color.
    pub fn fg(&self) -> Color {
        self.fg
    }

    /// Background color.
    pub fn bg(&self) -> Color {
        self.bg
    }

    /// Text attributes.
    pub fn attrs(&self) -> Attrs {
        self.attrs
    }
}

/// A cell color as the program set it. The deck maps it to design tokens; no hex here.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Color {
    /// The theme's default foreground or background.
    #[default]
    Default,
    /// Palette index: 0–15 the ANSI colors, 16–255 the xterm cube and grays.
    Indexed(u8),
    /// A true color.
    Rgb(u8, u8, u8),
}

/// Text attributes of a run: a small bitset.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Attrs(u8);

impl Attrs {
    /// No attribute.
    pub const NONE: Self = Self(0);
    /// Bold (SGR 1).
    pub const BOLD: Self = Self(1);
    /// Italic (SGR 3).
    pub const ITALIC: Self = Self(1 << 1);
    /// Any underline style (SGR 4, 4:x, 21).
    pub const UNDERLINE: Self = Self(1 << 2);
    /// Foreground and background swapped (SGR 7).
    pub const INVERSE: Self = Self(1 << 3);
    /// Dim (SGR 2).
    pub const DIM: Self = Self(1 << 4);

    /// Both sets of attributes.
    #[must_use]
    pub const fn with(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// True when every attribute of `other` is set.
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

/// Cursor position and visibility.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cursor {
    col: u16,
    row: u16,
    visible: bool,
}

impl Cursor {
    /// A cursor at `(col, row)`, zero-based.
    pub fn new(col: u16, row: u16, visible: bool) -> Self {
        Self { col, row, visible }
    }

    /// `(col, row)`, zero-based.
    pub fn position(&self) -> (u16, u16) {
        (self.col, self.row)
    }

    /// False while a program hides the cursor.
    pub fn visible(&self) -> bool {
        self.visible
    }
}

/// PTY size in cells.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Size {
    cols: u16,
    rows: u16,
}

impl Size {
    /// A size of `cols` by `rows` cells.
    #[cfg_attr(
        not(feature = "runner"),
        allow(dead_code, reason = "built by the runner only")
    )]
    pub(crate) fn new(cols: u16, rows: u16) -> Self {
        Self { cols, rows }
    }

    /// Columns.
    pub fn cols(&self) -> u16 {
        self.cols
    }

    /// Rows.
    pub fn rows(&self) -> u16 {
        self.rows
    }
}

/// The whole terminal state. The unit of sync (D31).
///
/// Scrollback syncs in full until a dedicated synced type exists (06-apps.md, State TODO).
#[derive(Debug, Default, PartialEq, Serialize, Deserialize, Synced)]
pub struct TermState {
    pub(crate) screen: List<Value<Row>>,
    pub(crate) scrollback: List<Value<Row>>,
    pub(crate) cursor: Value<Cursor>,
    pub(crate) size: Value<Size>,
    pub(crate) title: Value<String>,
}

impl TermState {
    /// Visible rows, top to bottom.
    pub fn screen(&self) -> &List<Value<Row>> {
        &self.screen
    }

    /// Rows above the screen, oldest first.
    pub fn scrollback(&self) -> &List<Value<Row>> {
        &self.scrollback
    }

    /// Cursor position and visibility.
    pub fn cursor(&self) -> Cursor {
        *self.cursor.get()
    }

    /// PTY size.
    pub fn size(&self) -> Size {
        *self.size.get()
    }

    /// Window title set by the shell or a program.
    pub fn title(&self) -> &str {
        self.title.get()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runs_carry_text_colors_and_attrs_for_viewers() {
        let run = Run::new(
            "x".to_owned(),
            Color::Indexed(1),
            Color::Rgb(1, 2, 3),
            Attrs::BOLD.with(Attrs::INVERSE),
        );
        assert_eq!(
            (run.text(), run.fg(), run.bg()),
            ("x", Color::Indexed(1), Color::Rgb(1, 2, 3))
        );
        assert!(run.attrs().contains(Attrs::INVERSE));
        assert!(!run.attrs().contains(Attrs::DIM));
        let cursor = Cursor::new(3, 4, true);
        assert_eq!((cursor.position(), cursor.visible()), ((3, 4), true));
    }
}

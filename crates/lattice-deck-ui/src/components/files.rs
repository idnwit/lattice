//! Files tool (right panel): a file tree with git marks.

use nsh_core::prelude::*;

use super::git::Mark;
use crate::style::{hrow, meta, mono, spacer};
use crate::tokens::{color, row, space, tint};

/// File tree indent per level (components.md: 12–14px).
const INDENT: f32 = space::SP_3;

/// A row of the file tree.
#[derive(Debug, Clone, PartialEq)]
pub struct FileRow {
    /// File or directory name.
    pub name: String,
    /// Nesting depth.
    pub depth: u8,
    /// `Some(open)` for a directory.
    pub dir: Option<bool>,
    /// Git mark, if changed (`●` for a dirty directory).
    pub mark: Option<Mark>,
}

/// The tree under `root` (`build-01:~/src/api`).
pub fn file_tree(root: &str, rows: &[FileRow]) -> View {
    view()
        .py(px(space::SP_2))
        .child(
            hrow(0.0)
                .h(px(row::ROW_SM - 2.0))
                .px(px(space::SP_3))
                .child(meta(root.to_owned())),
        )
        .children(rows.iter().map(file_row))
}

/// One tree row: caret, name, mark.
pub(crate) fn file_row(f: &FileRow) -> View {
    let caret = match f.dir {
        Some(true) => "▾",
        Some(false) => "▸",
        None => " ",
    };
    let mark = match (f.mark, f.dir.is_some()) {
        (None, _) => None,
        (Some(_), true) => Some(("●", tint::DIFF_MODIFIED)),
        (Some(m), false) => Some(m.glyph()),
    };
    let fg = if f.dir.is_some() {
        color::FG
    } else {
        color::FG_TERM
    };
    hrow(space::SP_1)
        .h(px(row::ROW_XS))
        .pl(px(space::SP_3 + INDENT * f32::from(f.depth)))
        .pr(px(space::SP_3))
        .child(meta(caret))
        .child(mono(f.name.clone(), fg))
        .child(spacer())
        .when_some(mark, |r, (m, hue)| r.child(meta(m).text_color(hue)))
}

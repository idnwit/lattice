//! Terminal area: static styled lines.
//!
//! The demo draws fixture lines as styled text runs rather than NSH's
//! `Terminal` element: that element paints ANSI colors from a fixed palette
//! of its own, while ours must come from `tokens.json`.

use nsh_core::prelude::*;

use super::style::{tone_color, typed};
use super::tokens::{color, space, typo};
use crate::model::{Density, Span, TermLine};

/// Compact terminal size: 12/18 (canvas, secondary panes and mobile).
const COMPACT: (f32, f32) = (12.0, 18.0);

/// A terminal showing `lines`, clipped to its box.
pub(crate) fn term_view(lines: &[TermLine], density: Density) -> View {
    view()
        .flex_1()
        .min_h(px(0.0))
        .overflow_hidden()
        // Newest lines stay in view; older ones scroll off the top.
        .justify_end()
        .px(px(space::SP_3))
        .py(px(space::SP_2))
        .bg(color::BG_0)
        .children(lines.iter().map(|l| term_line(l, density)))
}

fn term_line(line: &TermLine, density: Density) -> Text {
    let base = typed(text(""), typo::MONO_TERM)
        .text_color(color::FG_TERM)
        .whitespace_nowrap();
    let base = match density {
        Density::Regular => base,
        Density::Compact => base.text_size(px(COMPACT.0)).line_height(px(COMPACT.1)),
    };
    // An empty line still takes its row.
    let base = if line.spans.is_empty() || line.spans.iter().all(|s| s.text.is_empty()) {
        base.child(" ")
    } else {
        base.children(line.spans.iter().map(span))
    };
    base.when(line.cursor, |t| {
        t.child(text("█").text_color(color::ACCENT))
    })
}

fn span(s: &Span) -> Text {
    text(s.text.clone()).text_color(tone_color(s.tone))
}

/// The prompt line: `❯` and a real text field in the terminal's font.
///
/// `draft` holds the text; `submit` hears Enter with it. Focus stays in
/// the field after Enter, as in a shell.
pub(crate) fn prompt_input(
    name: &str,
    draft: Signal<String>,
    density: Density,
    submit: impl Fn(String) + 'static,
) -> View {
    let size = match density {
        Density::Regular => typo::MONO_TERM.size,
        Density::Compact => COMPACT.0,
    };
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

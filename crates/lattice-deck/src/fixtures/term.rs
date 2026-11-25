//! Terminal line builders and ANSI tones for fixtures.

use crate::model::{Span, TermLine, Tone};

pub(super) const GREEN: Tone = Tone::Ansi(2);
pub(super) const YELLOW: Tone = Tone::Ansi(3);
pub(super) const RED: Tone = Tone::Ansi(1);
pub(super) const PURPLE: Tone = Tone::Ansi(5);
pub(super) const CYAN: Tone = Tone::Ansi(6);

fn s(text: &str, tone: Tone) -> Span {
    Span {
        text: text.to_owned(),
        tone,
    }
}

pub(super) fn line(spans: &[(&str, Tone)]) -> TermLine {
    TermLine {
        spans: spans.iter().map(|(t, tone)| s(t, *tone)).collect(),
        cursor: false,
    }
}

pub(super) fn plain(text: &str) -> TermLine {
    line(&[(text, Tone::Term)])
}

pub(super) fn prompt(user: &str, branch: &str) -> TermLine {
    line(&[
        (user, GREEN),
        (" ", Tone::Term),
        ("~/src/api", CYAN),
        (" ", Tone::Term),
        (branch, Tone::Dim),
    ])
}

pub(super) fn command(cmd: &str) -> TermLine {
    line(&[("❯", Tone::Accent), (" ", Tone::Term), (cmd, Tone::Term)])
}

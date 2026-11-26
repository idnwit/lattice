//! Terminal line builders and ANSI tones for fixtures.

use crate::app::model::{Line, Run, Tone};

pub(super) const GREEN: Tone = Tone::Ansi(2);
pub(super) const YELLOW: Tone = Tone::Ansi(3);
pub(super) const RED: Tone = Tone::Ansi(1);
pub(super) const PURPLE: Tone = Tone::Ansi(5);
pub(super) const CYAN: Tone = Tone::Ansi(6);

fn s(text: &str, tone: Tone) -> Run {
    Run::new(text, tone)
}

pub(super) fn line(runs: &[(&str, Tone)]) -> Line {
    Line {
        runs: runs.iter().map(|(t, tone)| s(t, *tone)).collect(),
        cursor: false,
    }
}

pub(super) fn plain(text: &str) -> Line {
    line(&[(text, Tone::Text)])
}

pub(super) fn prompt(user: &str, branch: &str) -> Line {
    line(&[
        (user, GREEN),
        (" ", Tone::Text),
        ("~/src/api", CYAN),
        (" ", Tone::Text),
        (branch, Tone::Dim),
    ])
}

pub(super) fn command(cmd: &str) -> Line {
    line(&[("❯", Tone::Accent), (" ", Tone::Text), (cmd, Tone::Text)])
}

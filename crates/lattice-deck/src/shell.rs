//! Demo shell: what a typed command does to a terminal's lines.
//!
//! No process runs. A command echoes as `❯ cmd` and prints a canned
//! `demo: cmd` line; `clear` empties the screen.

use crate::model::{Span, TermLine, Tone};

/// Applies the command `input` to `lines`. Blank input does nothing.
pub(crate) fn run(lines: &mut Vec<TermLine>, input: &str) {
    let cmd = input.trim();
    if cmd.is_empty() {
        return;
    }
    // The fixture's last line may hold the idle prompt cursor.
    if let Some(last) = lines.last_mut() {
        last.cursor = false;
    }
    if cmd == "clear" {
        lines.clear();
        return;
    }
    lines.push(line(vec![span("❯ ", Tone::Accent), span(cmd, Tone::Term)]));
    lines.push(line(vec![span("demo: ", Tone::Dim), span(cmd, Tone::Term)]));
}

/// A note line: a colored `mark` (`✓`, `✗`) and muted text.
pub(crate) fn note(mark: &str, text: &str, tone: Tone) -> TermLine {
    line(vec![
        span(mark, tone),
        span(" ", tone),
        span(text, Tone::Muted),
    ])
}

fn span(text: &str, tone: Tone) -> Span {
    Span {
        text: text.to_owned(),
        tone,
    }
}

fn line(spans: Vec<Span>) -> TermLine {
    TermLine {
        spans,
        cursor: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(l: &TermLine) -> String {
        l.spans.iter().map(|s| s.text.as_str()).collect()
    }

    #[test]
    fn command_echoes_and_prints_demo_output() {
        let mut lines = vec![TermLine {
            spans: vec![],
            cursor: true,
        }];
        run(&mut lines, " ls -la ");
        let got: Vec<String> = lines.iter().map(text).collect();
        assert_eq!(got, ["", "❯ ls -la", "demo: ls -la"]);
        assert!(!lines.iter().any(|l| l.cursor), "old cursor goes");
    }

    #[test]
    fn clear_empties_and_blank_does_nothing() {
        let mut lines = vec![TermLine::default()];
        run(&mut lines, "   ");
        assert_eq!(lines.len(), 1, "case=blank");
        run(&mut lines, "clear");
        assert!(lines.is_empty(), "case=clear");
    }
}

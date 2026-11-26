//! Demo shell: what a typed command does to a terminal's lines.
//!
//! No process runs. A command echoes as `❯ cmd` and prints a canned
//! `demo: cmd` line; `clear` empties the screen.

use crate::app::model::{Line, Run, Tone};

/// Applies the command `input` to `lines`. Blank input does nothing.
pub(crate) fn run(lines: &mut Vec<Line>, input: &str) {
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
    lines.push(line(vec![span("❯ ", Tone::Accent), span(cmd, Tone::Text)]));
    lines.push(line(vec![span("demo: ", Tone::Dim), span(cmd, Tone::Text)]));
}

/// A note line: a colored `mark` (`✓`, `✗`) and muted text.
pub(crate) fn note(mark: &str, text: &str, tone: Tone) -> Line {
    line(vec![
        span(mark, tone),
        span(" ", tone),
        span(text, Tone::Muted),
    ])
}

fn span(text: &str, tone: Tone) -> Run {
    Run::new(text, tone)
}

fn line(runs: Vec<Run>) -> Line {
    Line {
        runs,
        cursor: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(l: &Line) -> String {
        l.runs.iter().map(|s| s.text.as_str()).collect()
    }

    #[test]
    fn command_echoes_and_prints_demo_output() {
        let mut lines = vec![Line {
            runs: vec![],
            cursor: true,
        }];
        run(&mut lines, " ls -la ");
        let got: Vec<String> = lines.iter().map(text).collect();
        assert_eq!(got, ["", "❯ ls -la", "demo: ls -la"]);
        assert!(!lines.iter().any(|l| l.cursor), "old cursor goes");
    }

    #[test]
    fn clear_empties_and_blank_does_nothing() {
        let mut lines = vec![Line::default()];
        run(&mut lines, "   ");
        assert_eq!(lines.len(), 1, "case=blank");
        run(&mut lines, "clear");
        assert!(lines.is_empty(), "case=clear");
    }
}

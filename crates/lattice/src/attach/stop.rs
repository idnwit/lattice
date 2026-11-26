//! Signals that end an attach (specs/03-sessions.md, Attach rules 9, 12).

use anyhow::Context;
use tokio::signal::unix::{Signal, SignalKind, signal};

/// `SIGTERM`, `SIGHUP`, and `SIGQUIT` detach, so the TTY guard restores the terminal (rule 12).
///
/// Raw mode turns off `ISIG`, so these arrive only from outside: `kill`, or a closed terminal.
pub(super) struct Stops {
    term: Signal,
    hup: Signal,
    quit: Signal,
}

impl Stops {
    /// Starts watching. Watch before raw mode, so no stop signal kills a raw TTY.
    pub(super) fn watch() -> anyhow::Result<Self> {
        Ok(Self {
            term: signal(SignalKind::terminate()).context("watch SIGTERM")?,
            hup: signal(SignalKind::hangup()).context("watch SIGHUP")?,
            quit: signal(SignalKind::quit()).context("watch SIGQUIT")?,
        })
    }

    /// Waits for any stop signal.
    pub(super) async fn recv(&mut self) {
        tokio::select! {
            _ = self.term.recv() => {}
            _ = self.hup.recv() => {}
            _ = self.quit.recv() => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[tokio::test]
    async fn term_hup_and_quit_each_stop() {
        let mut stops = Stops::watch().unwrap();
        let pid = std::process::id().to_string();
        for name in ["TERM", "HUP", "QUIT"] {
            let sent = std::process::Command::new("kill")
                .args([format!("-{name}"), pid.clone()])
                .status()
                .unwrap();
            assert!(sent.success(), "case={name}");
            let got = tokio::time::timeout(Duration::from_secs(5), stops.recv()).await;
            assert!(got.is_ok(), "case={name}");
        }
    }
}

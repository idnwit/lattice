//! The Add worker dialog as data (specs/01-pairing.md, Invite flow rules 2 and 11, UI rules
//! 2–7; patterns.md › Add worker). Sans-IO: the pairing task ([`crate::pairing`]) reports
//! to it, the views draw [`InviteDialog::view`].
//!
//! Each invite has a generation. A report for an older generation is ignored: `New invite`
//! replaced that invite.

use lattice_deck_core::{INVITE_TTL_SECS, PairStep};
use lattice_deck_ui::components::{AddWorker, Status};

/// Where the open invite is (rule 11).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum InviteStatus {
    /// No invite yet.
    Idle,
    /// The invite is being made (socket, route, secret).
    Making,
    /// `waiting for worker…`
    Waiting,
    /// `pairing…`: a worker's callback is being served.
    Pairing,
    /// `paired · <name>`
    Paired(String),
    /// `expired`
    Expired,
    /// The error code.
    Failed(String),
}

/// `lattice add-deck <invite>`. Holds the pairing secret: `Debug` redacts it.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct InviteCommand(pub(crate) String);

impl std::fmt::Debug for InviteCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("InviteCommand(<redacted>)")
    }
}

/// The dialog: shown or hidden, and the one open invite (rule 2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InviteDialog {
    shown: bool,
    generation: u64,
    command: Option<InviteCommand>,
    status: InviteStatus,
}

impl Default for InviteDialog {
    fn default() -> Self {
        Self {
            shown: false,
            generation: 0,
            command: None,
            status: InviteStatus::Idle,
        }
    }
}

impl InviteDialog {
    /// `+`: shows the dialog. `Some(generation)` when a new invite must be made; `None` when
    /// the open one is still waiting (UI rule 2).
    pub(crate) fn open(&mut self) -> Option<u64> {
        self.shown = true;
        match self.status {
            InviteStatus::Making | InviteStatus::Waiting | InviteStatus::Pairing => None,
            InviteStatus::Idle
            | InviteStatus::Paired(_)
            | InviteStatus::Expired
            | InviteStatus::Failed(_) => Some(self.next()),
        }
    }

    /// Close or `Escape`: hides the dialog; the invite stays open (UI rule 3).
    pub(crate) fn close(&mut self) {
        self.shown = false;
    }

    /// `New invite`: `Some(generation)` to make, once the invite expired or failed (UI rule 6).
    pub(crate) fn renew(&mut self) -> Option<u64> {
        match self.status {
            InviteStatus::Expired | InviteStatus::Failed(_) => Some(self.next()),
            _ => None,
        }
    }

    /// The invite of `generation` is made.
    pub(crate) fn made(&mut self, generation: u64, command: InviteCommand) {
        if self.is(generation) && self.status == InviteStatus::Making {
            self.command = Some(command);
            self.status = InviteStatus::Waiting;
        }
    }

    /// Progress of the invite of `generation` (UI rule 4).
    pub(crate) fn step(&mut self, generation: u64, step: PairStep) {
        if !self.is(generation) {
            return;
        }
        self.status = match (&self.status, step) {
            (InviteStatus::Waiting | InviteStatus::Pairing, PairStep::Serving) => {
                InviteStatus::Pairing
            }
            (InviteStatus::Waiting | InviteStatus::Pairing, PairStep::Waiting) => {
                InviteStatus::Waiting
            }
            (other, _) => other.clone(),
        };
    }

    /// The invite of `generation` paired with `worker`.
    pub(crate) fn paired(&mut self, generation: u64, worker: String) {
        self.end(generation, InviteStatus::Paired(worker));
    }

    /// The invite of `generation` expired (UI rule 5).
    pub(crate) fn expired(&mut self, generation: u64) {
        self.end(generation, InviteStatus::Expired);
    }

    /// Making or serving the invite of `generation` failed with `code`.
    pub(crate) fn failed(&mut self, generation: u64, code: String) {
        self.end(generation, InviteStatus::Failed(code));
    }

    /// Starts a new invite: the old one's reports no longer apply.
    fn next(&mut self) -> u64 {
        self.generation = self.generation.saturating_add(1);
        self.command = None;
        self.status = InviteStatus::Making;
        self.generation
    }

    fn is(&self, generation: u64) -> bool {
        generation == self.generation
    }

    /// A final state: the command can no longer pair, so it goes.
    fn end(&mut self, generation: u64, status: InviteStatus) {
        if self.is(generation) {
            self.command = None;
            self.status = status;
        }
    }

    /// True while the dialog shows.
    pub(crate) fn shown(&self) -> bool {
        self.shown
    }

    /// The command to copy, while the invite can pair.
    pub(crate) fn command(&self) -> Option<&InviteCommand> {
        self.command.as_ref()
    }

    /// The invite's state.
    #[cfg(test)]
    pub(crate) fn status(&self) -> &InviteStatus {
        &self.status
    }

    /// What the dialog draws: glyph + text per patterns.md › Add worker.
    pub(crate) fn view(&self) -> AddWorker {
        let waiting = || (Status::Waiting, "waiting for worker…".to_owned());
        let (status, status_text) = match &self.status {
            InviteStatus::Idle | InviteStatus::Making | InviteStatus::Waiting => waiting(),
            InviteStatus::Pairing => (Status::Idle, "pairing…".to_owned()),
            InviteStatus::Paired(name) => (Status::Online, format!("paired · {name}")),
            InviteStatus::Expired => (Status::Offline, "expired".to_owned()),
            InviteStatus::Failed(code) => (Status::Error, code.clone()),
        };
        AddWorker {
            command: self.command.as_ref().map(|c| c.0.clone()),
            status,
            status_text,
            renew: matches!(self.status, InviteStatus::Expired | InviteStatus::Failed(_)),
            ttl_mins: INVITE_TTL_SECS / 60,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmd() -> InviteCommand {
        InviteCommand("lattice add-deck lattice1:abc".to_owned())
    }

    /// A dialog whose first invite is made and waiting.
    fn waiting() -> (InviteDialog, u64) {
        let mut d = InviteDialog::default();
        let g = d.open().expect("the first open makes an invite");
        d.made(g, cmd());
        (d, g)
    }

    fn shown(d: &InviteDialog) -> (Status, String, bool, bool) {
        let v = d.view();
        (v.status, v.status_text, v.command.is_some(), v.renew)
    }

    #[test]
    fn status_line_follows_the_invite() {
        let (mut d, g) = waiting();
        let s = |st: Status, t: &str, cmd: bool, renew: bool| (st, t.to_owned(), cmd, renew);
        assert_eq!(
            shown(&d),
            s(Status::Waiting, "waiting for worker…", true, false)
        );
        d.step(g, PairStep::Serving);
        assert_eq!(shown(&d), s(Status::Idle, "pairing…", true, false));
        d.step(g, PairStep::Waiting);
        assert_eq!(
            shown(&d),
            s(Status::Waiting, "waiting for worker…", true, false)
        );
        d.paired(g, "build-01".to_owned());
        assert_eq!(
            shown(&d),
            s(Status::Online, "paired · build-01", false, false)
        );

        let (mut d, g) = waiting();
        d.expired(g);
        assert_eq!(shown(&d), s(Status::Offline, "expired", false, true));

        let (mut d, g) = waiting();
        d.failed(g, "transport.io".to_owned());
        assert_eq!(shown(&d), s(Status::Error, "transport.io", false, true));
    }

    #[test]
    fn closing_keeps_a_waiting_invite_and_plus_shows_it_again() {
        let (mut d, g) = waiting();
        d.close();
        assert!(!d.shown());
        assert_eq!(d.open(), None, "the waiting invite is reused");
        assert!(d.shown());
        assert_eq!(d.command(), Some(&cmd()));
        d.paired(g, "build-01".to_owned());
        let next = d.open();
        assert!(next.is_some_and(|n| n > g), "a paired invite is spent");
    }

    #[test]
    fn new_invite_only_once_expired_or_failed_and_old_reports_are_ignored() {
        let (mut d, g) = waiting();
        assert_eq!(d.renew(), None, "a waiting invite is not replaced");
        d.expired(g);
        let g2 = d.renew().expect("expired invites renew");
        assert!(g2 > g);
        assert_eq!(d.status(), &InviteStatus::Making);
        d.paired(g, "late".to_owned());
        d.step(g, PairStep::Serving);
        assert_eq!(
            d.status(),
            &InviteStatus::Making,
            "reports of the old invite are ignored"
        );
        d.failed(g2, "pair.no_route".to_owned());
        assert!(d.renew().is_some());
    }

    #[test]
    fn the_command_never_shows_in_debug() {
        let (d, _) = waiting();
        let text = format!("{d:?} {:?}", d.view());
        assert!(!text.contains("lattice1:"), "{text}");
    }
}

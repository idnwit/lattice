//! Stdin bytes and `worker/apps` lists as attach decisions (specs/03-sessions.md, Attach). Sans-IO.

use lattice_app::InstancePhase;

/// The detach key: `Ctrl-\` (rule 8).
pub(super) const DETACH_KEY: u8 = 0x1C;

/// What one stdin read means.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Keys<'a> {
    /// Bytes for the instance `input`.
    Input(&'a [u8]),
    /// The detach key; `before` are the bytes typed ahead of it, sent first.
    Detach {
        /// Bytes before the detach key.
        before: &'a [u8],
    },
}

/// Splits one stdin read at the first detach key (rules 7, 8).
pub(super) fn keys(read: &[u8]) -> Keys<'_> {
    match read.iter().position(|&b| b == DETACH_KEY) {
        Some(at) => Keys::Detach {
            before: read.get(..at).unwrap_or_default(),
        },
        None => Keys::Input(read),
    }
}

/// How the attached instance came to be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Origin {
    /// `lattice attach` created it.
    Created,
    /// `--iid` named it.
    Existing,
}

/// Why the attach ends, read from `worker/apps` (rules 2, 10, 17).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Gone {
    /// The instance was in a list and left it.
    Exited,
    /// `--iid` named an instance the first list lacks.
    Missing,
    /// The instance is listed in phase `failed`. It stays listed; the client never closes it.
    Failed,
}

/// Tracks the attached instance across `worker/apps` lists.
#[derive(Debug)]
pub(super) struct Presence {
    origin: Origin,
    seen: bool,
    heard: bool,
}

impl Presence {
    /// Nothing seen yet.
    pub(super) fn new(origin: Origin) -> Self {
        Self {
            origin,
            seen: false,
            heard: false,
        }
    }

    /// True once a `worker/apps` list arrived (rule 18).
    pub(super) fn heard(&self) -> bool {
        self.heard
    }

    /// One full `worker/apps` list; `phase`: the instance's phase there, `None` when unlisted.
    pub(super) fn on_list(&mut self, phase: Option<InstancePhase>) -> Option<Gone> {
        self.heard = true;
        if let Some(phase) = phase {
            self.seen = true;
            return (phase == InstancePhase::Failed).then_some(Gone::Failed);
        }
        match (self.seen, self.origin) {
            (true, _) => Some(Gone::Exited),
            (false, Origin::Existing) => Some(Gone::Missing),
            // `create` answered before the list caught up: wait for the next one.
            (false, Origin::Created) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stdin_bytes_go_to_input_unchanged() {
        let cases: [(&str, &[u8]); 4] = [
            ("text", b"echo hi\r"),
            ("ctrl-c", b"\x03"),
            ("arrow", b"\x1b[A"),
            ("utf8", "é".as_bytes()),
        ];
        for (name, read) in cases {
            assert_eq!(keys(read), Keys::Input(read), "case={name}");
        }
    }

    #[test]
    fn detach_key_detaches_after_sending_bytes_before_it() {
        assert_eq!(keys(b"\x1c"), Keys::Detach { before: b"" });
        assert_eq!(keys(b"ls\x1cpwd"), Keys::Detach { before: b"ls" });
        assert_eq!(
            keys(&[b'a', DETACH_KEY, DETACH_KEY]),
            Keys::Detach { before: b"a" }
        );
    }

    const RUNNING: Option<InstancePhase> = Some(InstancePhase::Running);
    const FAILED: Option<InstancePhase> = Some(InstancePhase::Failed);

    #[test]
    fn created_instance_ends_only_after_it_was_listed() {
        let mut p = Presence::new(Origin::Created);
        assert!(!p.heard());
        assert_eq!(p.on_list(None), None, "not listed yet");
        assert!(p.heard());
        assert_eq!(p.on_list(Some(InstancePhase::Starting)), None);
        assert_eq!(p.on_list(RUNNING), None);
        assert_eq!(p.on_list(None), Some(Gone::Exited));
    }

    #[test]
    fn existing_instance_missing_from_first_list_is_missing() {
        let mut p = Presence::new(Origin::Existing);
        assert_eq!(p.on_list(None), Some(Gone::Missing));
        let mut p = Presence::new(Origin::Existing);
        assert_eq!(p.on_list(RUNNING), None);
        assert_eq!(p.on_list(None), Some(Gone::Exited));
    }

    /// Attach rule 17: `failed` ends the attach, in the first list or a later one.
    #[test]
    fn a_failed_instance_ends_the_attach_as_failed() {
        let cases = [
            ("existing, first list", Origin::Existing, vec![FAILED]),
            ("created, first list", Origin::Created, vec![FAILED]),
            (
                "created, spawn fails later",
                Origin::Created,
                vec![RUNNING, FAILED],
            ),
            (
                "existing, fails later",
                Origin::Existing,
                vec![RUNNING, FAILED],
            ),
        ];
        for (name, origin, lists) in cases {
            let mut p = Presence::new(origin);
            let got: Vec<Option<Gone>> = lists.into_iter().map(|l| p.on_list(l)).collect();
            assert_eq!(got.last(), Some(&Some(Gone::Failed)), "case={name}");
        }
    }

    #[test]
    fn closing_and_degraded_instances_stay_attached() {
        for phase in [InstancePhase::Closing, InstancePhase::Degraded] {
            let mut p = Presence::new(Origin::Existing);
            assert_eq!(p.on_list(Some(phase)), None, "case={phase:?}");
        }
    }
}

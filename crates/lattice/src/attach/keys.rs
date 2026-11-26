//! Stdin bytes and `worker/apps` lists as attach decisions (specs/03-sessions.md, Attach). Sans-IO.

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

/// Why the attach ends, read from `worker/apps` (rules 2, 10).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Gone {
    /// The instance was in a list and left it.
    Exited,
    /// `--iid` named an instance the first list lacks.
    Missing,
}

/// Tracks the attached instance across `worker/apps` lists.
#[derive(Debug)]
pub(super) struct Presence {
    origin: Origin,
    seen: bool,
}

impl Presence {
    /// Nothing seen yet.
    pub(super) fn new(origin: Origin) -> Self {
        Self {
            origin,
            seen: false,
        }
    }

    /// One full `worker/apps` list; `listed`: the instance is in it.
    pub(super) fn on_list(&mut self, listed: bool) -> Option<Gone> {
        if listed {
            self.seen = true;
            return None;
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

    #[test]
    fn created_instance_ends_only_after_it_was_listed() {
        let mut p = Presence::new(Origin::Created);
        assert_eq!(p.on_list(false), None, "not listed yet");
        assert_eq!(p.on_list(true), None);
        assert_eq!(p.on_list(true), None);
        assert_eq!(p.on_list(false), Some(Gone::Exited));
    }

    #[test]
    fn existing_instance_missing_from_first_list_is_missing() {
        let mut p = Presence::new(Origin::Existing);
        assert_eq!(p.on_list(false), Some(Gone::Missing));
        let mut p = Presence::new(Origin::Existing);
        assert_eq!(p.on_list(true), None);
        assert_eq!(p.on_list(false), Some(Gone::Exited));
    }
}

//! Effects a reducer records (06-apps.md, Runner rule 1).

use serde::{Deserialize, Serialize};

use crate::ids::ProcRef;

/// Signals a runner may send. Closed set; `close` and `kill` are host actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Signal {
    /// `SIGINT`.
    Interrupt,
    /// `SIGHUP`.
    Hangup,
    /// `SIGTERM`.
    Terminate,
}

/// One effect. The host runs it after commit, in record order, and drops it during replay.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Effect {
    /// Spawn from the `[[spawn]]` template with this name. The manifest fixes argv; the
    /// runner picks only where and how big.
    Spawn {
        /// Template name from `app.toml`.
        template: String,
        /// Working directory; empty means the user's home.
        cwd: String,
        /// Initial PTY size (columns, rows); `None` means the host default.
        size: Option<(u16, u16)>,
    },
    /// Write bytes to a process PTY.
    Write {
        /// Target process.
        proc: ProcRef,
        /// Raw bytes.
        bytes: Vec<u8>,
    },
    /// Resize a process PTY.
    Resize {
        /// Target process.
        proc: ProcRef,
        /// Columns.
        cols: u16,
        /// Rows.
        rows: u16,
    },
    /// Signal a process.
    Signal {
        /// Target process.
        proc: ProcRef,
        /// Which signal.
        signal: Signal,
    },
    /// Drop journal history before the entry being reduced (a clean point).
    Trim,
    /// End the instance: the host closes it without confirmation.
    Finish,
}

/// Records effects for one reducer call.
#[derive(Debug, Default)]
pub struct Ctx {
    effects: Vec<Effect>,
}

impl Ctx {
    /// An empty context.
    pub fn new() -> Self {
        Self::default()
    }

    /// Spawns from a `[[spawn]]` template in the user's home, at the host's default size.
    pub fn spawn(&mut self, template: &str) {
        self.effects.push(Effect::Spawn {
            template: template.to_owned(),
            cwd: String::new(),
            size: None,
        });
    }

    /// Spawns from a `[[spawn]]` template in `cwd` (empty: home) with an initial PTY size.
    pub fn spawn_at(&mut self, template: &str, cwd: &str, cols: u16, rows: u16) {
        self.effects.push(Effect::Spawn {
            template: template.to_owned(),
            cwd: cwd.to_owned(),
            size: Some((cols, rows)),
        });
    }

    /// Writes bytes to a process.
    pub fn write(&mut self, proc: ProcRef, bytes: Vec<u8>) {
        self.effects.push(Effect::Write { proc, bytes });
    }

    /// Resizes a process PTY.
    pub fn resize(&mut self, proc: ProcRef, cols: u16, rows: u16) {
        self.effects.push(Effect::Resize { proc, cols, rows });
    }

    /// Signals a process.
    pub fn signal(&mut self, proc: ProcRef, signal: Signal) {
        self.effects.push(Effect::Signal { proc, signal });
    }

    /// Marks the current entry as a clean point.
    pub fn trim(&mut self) {
        self.effects.push(Effect::Trim);
    }

    /// Ends the instance.
    pub fn finish(&mut self) {
        self.effects.push(Effect::Finish);
    }

    /// Takes the recorded effects. The host calls it after commit.
    pub fn take_effects(&mut self) -> Vec<Effect> {
        std::mem::take(&mut self.effects)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effects_come_out_in_record_order() {
        let p = ProcRef::from_raw(1);
        let mut ctx = Ctx::new();
        ctx.spawn("shell");
        ctx.spawn_at("shell", "/tmp", 120, 40);
        ctx.write(p, b"ls\r".to_vec());
        ctx.resize(p, 120, 40);
        ctx.signal(p, Signal::Interrupt);
        ctx.trim();
        ctx.finish();
        let want = vec![
            Effect::Spawn {
                template: "shell".to_owned(),
                cwd: String::new(),
                size: None,
            },
            Effect::Spawn {
                template: "shell".to_owned(),
                cwd: "/tmp".to_owned(),
                size: Some((120, 40)),
            },
            Effect::Write {
                proc: p,
                bytes: b"ls\r".to_vec(),
            },
            Effect::Resize {
                proc: p,
                cols: 120,
                rows: 40,
            },
            Effect::Signal {
                proc: p,
                signal: Signal::Interrupt,
            },
            Effect::Trim,
            Effect::Finish,
        ];
        assert_eq!(ctx.take_effects(), want);
        assert!(ctx.take_effects().is_empty());
    }
}

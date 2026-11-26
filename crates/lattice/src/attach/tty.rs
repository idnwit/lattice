//! The local terminal: TTY check, size, raw mode (specs/03-sessions.md, Attach rules 3, 5, 6, 12).

use std::io::Write;
use std::panic::PanicHookInfo;
use std::sync::Arc;

use anyhow::Context;
use rustix::termios::{OptionalActions, Termios, isatty, tcgetattr, tcgetwinsize, tcsetattr};

use super::paint::{ENTER, LEAVE};

/// Columns used when the TTY reports zero (rule 5).
const FALLBACK_COLS: u16 = 80;
/// Rows used when the TTY reports zero (rule 5).
const FALLBACK_ROWS: u16 = 24;

/// Proof that stdin and stdout are both a TTY (rule 3).
#[derive(Debug)]
pub(super) struct Tty(());

impl Tty {
    /// Checks stdin and stdout.
    pub(super) fn check() -> anyhow::Result<Self> {
        if !isatty(std::io::stdin()) || !isatty(std::io::stdout()) {
            anyhow::bail!("lattice attach needs a terminal on stdin and stdout");
        }
        Ok(Self(()))
    }

    /// `(cols, rows)` of the local terminal.
    pub(super) fn size(&self) -> (u16, u16) {
        let ws = tcgetwinsize(std::io::stdout()).ok();
        usable(ws.map_or((0, 0), |w| (w.ws_col, w.ws_row)))
    }

    /// Puts the TTY in raw mode on the alternate screen until the guard drops (rules 6, 12).
    pub(super) fn raw(&self) -> anyhow::Result<RawMode> {
        let saved = tcgetattr(std::io::stdin()).context("read tty mode")?;
        let mut raw = saved.clone();
        raw.make_raw();
        tcsetattr(std::io::stdin(), OptionalActions::Now, &raw).context("set raw mode")?;
        let mut out = std::io::stdout().lock();
        out.write_all(ENTER)
            .and_then(|()| out.flush())
            .context("enter screen")?;
        // A panic message prints before unwinding drops the guard: restore first so it shows.
        let on_panic = saved.clone();
        let hook = PanicHook::install(move || restore(&on_panic));
        Ok(RawMode { saved, _hook: hook })
    }
}

/// Raw mode on; dropping it restores the TTY and the previous panic hook (rule 12).
pub(super) struct RawMode {
    saved: Termios,
    _hook: PanicHook,
}

impl Drop for RawMode {
    fn drop(&mut self) {
        restore(&self.saved);
    }
}

/// The panic hook in place before [`PanicHook::install`].
type PrevHook = Box<dyn Fn(&PanicHookInfo<'_>) + Send + Sync + 'static>;

/// Runs `on_panic` before the previous panic hook until dropped. Drop puts the previous hook back.
struct PanicHook {
    prev: Arc<PrevHook>,
}

impl PanicHook {
    fn install(on_panic: impl Fn() + Send + Sync + 'static) -> Self {
        let prev = Arc::new(std::panic::take_hook());
        let chained = Arc::clone(&prev);
        std::panic::set_hook(Box::new(move |info| {
            on_panic();
            chained(info);
        }));
        Self { prev }
    }
}

impl Drop for PanicHook {
    fn drop(&mut self) {
        // The hook cannot change while panicking. Ours already ran, and the process is unwinding.
        if std::thread::panicking() {
            return;
        }
        drop(std::panic::take_hook());
        let prev = Arc::clone(&self.prev);
        std::panic::set_hook(Box::new(move |info| prev(info)));
    }
}

/// Leaves the alternate screen and restores `saved`. Idempotent.
fn restore(saved: &Termios) {
    let mut out = std::io::stdout().lock();
    // Best effort on the way out: a closed stdout has no screen left to fix.
    drop(out.write_all(LEAVE).and_then(|()| out.flush()));
    // Same: if the TTY is gone there is no mode to restore.
    tcsetattr(std::io::stdin(), OptionalActions::Now, saved).unwrap_or_default();
}

/// A size `resize` accepts: a zero side becomes 80 by 24 (rule 5).
fn usable((cols, rows): (u16, u16)) -> (u16, u16) {
    if cols == 0 || rows == 0 {
        return (FALLBACK_COLS, FALLBACK_ROWS);
    }
    (cols, rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_tty_size_falls_back_to_80_by_24() {
        let cases = [
            ("real", (120, 40), (120, 40)),
            ("zero", (0, 0), (80, 24)),
            ("zero cols", (0, 40), (80, 24)),
            ("zero rows", (120, 0), (80, 24)),
        ];
        for (name, got, want) in cases {
            assert_eq!(usable(got), want, "case={name}");
        }
    }

    #[test]
    fn dropped_panic_hook_stops_restoring() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let base = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let runs = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&runs);
        let hook = PanicHook::install(move || {
            counted.fetch_add(1, Ordering::SeqCst);
        });
        drop(std::panic::catch_unwind(|| {
            std::panic::panic_any("installed")
        }));
        let while_installed = runs.load(Ordering::SeqCst);
        drop(hook);
        drop(std::panic::catch_unwind(|| {
            std::panic::panic_any("dropped")
        }));
        let after_drop = runs.load(Ordering::SeqCst);
        std::panic::set_hook(base);
        assert_eq!((while_installed, after_drop), (1, 1));
    }
}

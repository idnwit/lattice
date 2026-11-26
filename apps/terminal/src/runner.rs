//! Worker side: reducers of the terminal app (feature `runner`).
//!
//! Rules:
//! - **Shell binding.** The terminal spawns exactly one process, `shell`. The host
//!   does not tell the runner its [`ProcRef`], so the first proc seen in `on_output`
//!   or `on_exit` is the shell. Until then `input` fails with [`TermError::NoShell`];
//!   `resize` still resizes the emulator and the state, and the PTY resize is sent
//!   when the shell binds. Output of any other proc is ignored.
//! - **Scrollback.** Lines leaving the top of the emulator's screen are drained after
//!   every [`vt::SEGMENT_BYTES`] fed and pushed to `TermState::scrollback`. The list
//!   is capped at [`SCROLLBACK_CAP`] rows: on overflow it keeps the newest half
//!   (one delta of `truncate 0` + push), since `List` only cuts from the end.
//! - **Clean point.** `ESC[3J` processed outside the alternate screen and outside a
//!   synchronized update clears `scrollback`. If the screen is blank right after it
//!   (as after `clear`: `ESC[H ESC[2J ESC[3J`), it is also a clean point: `trim`.
//! - **Resumed.** After restore (and right after `start`) a bound, live shell gets a
//!   PTY resize to the current size: a nudge so full-screen programs redraw.
//! - **Replies.** Answers the emulator owes the program (cursor position report,
//!   device attributes) are written to the shell; dropped on replay like any effect.

mod vt;

use lattice_app::{App, Ctx, ExitStatus, ProcRef};
use lattice_state::{List, Value};

use crate::api::{Input, Resize, TermError, TermParams, TerminalApi};
use crate::model::{Row, Size, TermState};
use vt::{Note, Vt};

/// Most rows kept in `scrollback`. TBD in 06-apps.md (State TODO: a capped synced type).
pub const SCROLLBACK_CAP: usize = 10_000;

/// `ESC[3J`: erase saved lines.
const CLEAR_SAVED: &[u8] = b"\x1b[3J";

/// Where the shell is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shell {
    /// No output or exit seen yet.
    Unbound,
    /// Running.
    Live(ProcRef),
    /// Exited.
    Exited(ProcRef),
}

/// Runner of one terminal instance. Holds the VT emulator (private memory).
///
/// TBD: the emulator (`vt100`) lives in adapter `lattice-vt`, and apps
/// may not reach adapters (.rules/architecture.md). Inject it through a trait, or
/// move the VT parser into a core crate? For now the app depends on it directly.
#[derive(Debug)]
pub struct Terminal {
    vt: Vt,
    shell: Shell,
    /// Size of the emulator; the PTY gets it when the shell binds.
    size: Size,
    /// A resize came before the shell bound: send it on bind.
    pty_stale: bool,
    /// Bytes of [`CLEAR_SAVED`] matched at the end of the output so far.
    clear_match: usize,
    /// Title from the last title note.
    title: String,
    /// Where the shell starts (params); empty means home.
    cwd: String,
}

impl App for Terminal {
    const ID: &'static str = crate::APP_ID;
    type State = TermState;
    type Params = TermParams;
    type Error = TermError;

    fn new(params: &TermParams) -> Self {
        let (cols, rows) = params.size();
        Self {
            vt: Vt::new(cols, rows),
            shell: Shell::Unbound,
            size: Size::new(cols, rows),
            pty_stale: false,
            clear_match: 0,
            title: String::new(),
            cwd: params.cwd().to_owned(),
        }
    }

    fn start(&mut self, ctx: &mut Ctx, state: &mut TermState) -> Result<(), TermError> {
        ctx.spawn_at("shell", &self.cwd, self.size.cols(), self.size.rows());
        state.size.set(self.size);
        self.sync_screen(state);
        Ok(())
    }

    fn on_output(&mut self, ctx: &mut Ctx, state: &mut TermState, proc: ProcRef, bytes: &[u8]) {
        if !self.is_shell(ctx, proc) {
            return;
        }
        let mut rest = bytes;
        while !rest.is_empty() {
            let (segment, cleared) = self.next_segment(rest);
            rest = rest.get(segment.len()..).unwrap_or_default();
            self.vt.feed(segment);
            push_scrollback(&mut state.scrollback, self.vt.drain_history());
            if cleared {
                self.on_clear_saved(ctx, state);
            }
        }
        self.sync_screen(state);
        self.handle_notes(ctx, state);
    }

    fn on_exit(&mut self, ctx: &mut Ctx, _state: &mut TermState, proc: ProcRef, _: ExitStatus) {
        if self.is_shell(ctx, proc) {
            self.shell = Shell::Exited(proc);
            ctx.finish();
        }
    }

    fn on_gap(&mut self, _ctx: &mut Ctx, state: &mut TermState) {
        // History before this point is gone: start the emulator over. Scrollback stays as rebuilt.
        self.vt = Vt::new(self.size.cols(), self.size.rows());
        self.clear_match = 0;
        self.title.clear();
        self.sync_screen(state);
        state.title.set(String::new());
    }

    fn resumed(&mut self, ctx: &mut Ctx, _state: &mut TermState) {
        if let Shell::Live(shell) = self.shell {
            ctx.resize(shell, self.size.cols(), self.size.rows());
        }
    }
}

impl TerminalApi for Terminal {
    fn input(
        &mut self,
        ctx: &mut Ctx,
        _state: &mut TermState,
        req: Input,
    ) -> Result<(), TermError> {
        match self.shell {
            Shell::Live(shell) => {
                ctx.write(shell, req.into_bytes());
                Ok(())
            }
            Shell::Unbound | Shell::Exited(_) => Err(TermError::NoShell),
        }
    }

    fn resize(
        &mut self,
        ctx: &mut Ctx,
        state: &mut TermState,
        req: Resize,
    ) -> Result<Size, TermError> {
        self.resize_to(ctx, state, req.cols(), req.rows())?;
        Ok(self.size)
    }
}

impl Terminal {
    /// Binds the shell on the first proc seen; true when `proc` is the shell.
    fn is_shell(&mut self, ctx: &mut Ctx, proc: ProcRef) -> bool {
        match self.shell {
            Shell::Unbound => {
                self.shell = Shell::Live(proc);
                if std::mem::take(&mut self.pty_stale) {
                    ctx.resize(proc, self.size.cols(), self.size.rows());
                }
                true
            }
            Shell::Live(shell) | Shell::Exited(shell) => shell == proc,
        }
    }

    fn resize_to(
        &mut self,
        ctx: &mut Ctx,
        state: &mut TermState,
        cols: u16,
        rows: u16,
    ) -> Result<(), TermError> {
        if cols == 0 || rows == 0 {
            return Err(TermError::BadSize);
        }
        match self.shell {
            Shell::Exited(_) => return Err(TermError::NoShell),
            Shell::Live(shell) => ctx.resize(shell, cols, rows),
            Shell::Unbound => self.pty_stale = true,
        }
        self.size = Size::new(cols, rows);
        self.vt.resize(cols, rows);
        push_scrollback(&mut state.scrollback, self.vt.drain_history());
        state.size.set(self.size);
        self.sync_screen(state);
        Ok(())
    }

    /// The next bytes to feed: up to [`vt::SEGMENT_BYTES`], cut right after a
    /// [`CLEAR_SAVED`] (true then). The match carries across calls, so chunking does not matter.
    fn next_segment<'a>(&mut self, bytes: &'a [u8]) -> (&'a [u8], bool) {
        for (i, byte) in bytes.iter().take(vt::SEGMENT_BYTES).enumerate() {
            self.clear_match = match CLEAR_SAVED.get(self.clear_match) {
                Some(want) if want == byte => self.clear_match.saturating_add(1),
                // No proper prefix of `ESC[3J` is also a suffix, except a lone ESC.
                _ => usize::from(*byte == 0x1b),
            };
            if self.clear_match == CLEAR_SAVED.len() {
                self.clear_match = 0;
                return (bytes.get(..=i).unwrap_or(bytes), true);
            }
        }
        (bytes.get(..vt::SEGMENT_BYTES).unwrap_or(bytes), false)
    }

    /// `ESC[3J` was just fed.
    fn on_clear_saved(&mut self, ctx: &mut Ctx, state: &mut TermState) {
        // Alternate screen: the main scrollback is untouched. Sync update: not applied yet.
        if self.vt.in_alt_screen() || self.vt.syncing() {
            return;
        }
        state.scrollback.clear();
        self.sync_screen(state);
        if state.screen.iter().all(|row| row.get().runs().is_empty()) {
            ctx.trim();
        }
    }

    /// Writes changed screen rows and the cursor into the state. Unchanged rows stay
    /// out of the draft, so they cost nothing in the delta.
    fn sync_screen(&self, state: &mut TermState) {
        let rows = self.vt.rows();
        state.screen.truncate(rows);
        while state.screen.len() < rows {
            state.screen.push(Value::new(Row::default()));
        }
        for index in 0..rows {
            let row = self.vt.row(index);
            if state.screen.get(index).map(Value::get) != Some(&row)
                && let Some(slot) = state.screen.get_mut(index)
            {
                slot.set(row);
            }
        }
        state.cursor.set(self.vt.cursor());
    }

    fn handle_notes(&mut self, ctx: &mut Ctx, state: &mut TermState) {
        for note in self.vt.take_notes() {
            match note {
                Note::Title(title) => self.title = title.unwrap_or_default(),
                Note::Reply(text) => {
                    if let Shell::Live(shell) = self.shell {
                        ctx.write(shell, text.into_bytes());
                    }
                }
            }
        }
        if state.title.get() != &self.title {
            state.title.set(self.title.clone());
        }
    }
}

/// Appends rows to scrollback; past [`SCROLLBACK_CAP`] keeps the newest half.
fn push_scrollback(list: &mut List<Value<Row>>, rows: Vec<Row>) {
    for row in rows {
        if list.len() >= SCROLLBACK_CAP {
            let keep = SCROLLBACK_CAP / 2;
            let newest: Vec<Row> = list
                .iter()
                .skip(list.len().saturating_sub(keep))
                .map(|row| row.get().clone())
                .collect();
            list.clear();
            newest
                .into_iter()
                .for_each(|row| list.push(Value::new(row)));
        }
        list.push(Value::new(row));
    }
}

#[cfg(test)]
mod tests;

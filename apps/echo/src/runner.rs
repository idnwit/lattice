//! Worker side: reducers of the echo app (feature `runner`).
//!
//! Rules:
//! - **No process.** `start` spawns nothing; `on_output`, `on_exit`, and `on_gap` never
//!   fire for an instance without processes, and do nothing if they do.
//! - **Sequence.** The next sequence number is private runner memory, starting at 1.
//!   It moves only when `send` commits, so a refused call leaves no hole. `clear` does
//!   not reset it. A restart does (see the crate doc, Restart).
//! - **Cap.** `lines` holds at most [`LINES_CAP`] lines, oldest first. `List` only cuts
//!   from the end, so a `send` that finds the list full rebuilds it with the newest
//!   `LINES_CAP / 2 - 1` lines, then pushes: one delta of `truncate 0` + push, once per
//!   `LINES_CAP / 2` sends, instead of a full rebuild on every send.

use lattice_app::{App, Ctx, ExitStatus, ProcRef};
use lattice_state::{List, Value};

use crate::api::{EchoApi, EchoError, EchoParams, MAX_TEXT_BYTES, Message};
use crate::model::{EchoState, Line};

/// Most lines kept in [`EchoState`]. TBD in 06-apps.md (State TODO: a capped synced type).
pub const LINES_CAP: usize = 1_000;

/// Runner of one echo instance.
#[derive(Debug)]
pub struct Echo {
    /// Sequence number of the next accepted `send`.
    next_seq: u64,
}

impl App for Echo {
    const ID: &'static str = crate::APP_ID;
    type State = EchoState;
    type Params = EchoParams;
    type Error = EchoError;

    fn new(_params: &EchoParams) -> Self {
        Self { next_seq: 1 }
    }

    fn start(&mut self, _ctx: &mut Ctx, _state: &mut EchoState) -> Result<(), EchoError> {
        Ok(())
    }

    fn on_output(&mut self, _: &mut Ctx, _: &mut EchoState, _: ProcRef, _: &[u8]) {}

    fn on_exit(&mut self, _: &mut Ctx, _: &mut EchoState, _: ProcRef, _: ExitStatus) {}

    fn on_gap(&mut self, _ctx: &mut Ctx, _state: &mut EchoState) {}
}

impl EchoApi for Echo {
    fn send(
        &mut self,
        _ctx: &mut Ctx,
        state: &mut EchoState,
        req: Message,
    ) -> Result<u64, EchoError> {
        if req.text().trim().is_empty() {
            return Err(EchoError::Empty);
        }
        if req.text().len() > MAX_TEXT_BYTES {
            return Err(EchoError::TooLong);
        }
        let seq = self.next_seq;
        push_capped(&mut state.lines, Line::new(seq, req.into_text()));
        self.next_seq = seq.saturating_add(1);
        Ok(seq)
    }

    fn clear(&mut self, _ctx: &mut Ctx, state: &mut EchoState, (): ()) -> Result<(), EchoError> {
        state.lines.clear();
        Ok(())
    }
}

/// Appends `line`; a full list first keeps only its newest `LINES_CAP / 2 - 1` lines.
fn push_capped(lines: &mut List<Value<Line>>, line: Line) {
    if lines.len() >= LINES_CAP {
        let keep = (LINES_CAP / 2).saturating_sub(1);
        let newest: Vec<Line> = lines
            .iter()
            .skip(lines.len().saturating_sub(keep))
            .map(|l| l.get().clone())
            .collect();
        lines.clear();
        newest.into_iter().for_each(|l| lines.push(Value::new(l)));
    }
    lines.push(Value::new(line));
}

#[cfg(test)]
mod tests;

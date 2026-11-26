//! `#[interface]` end to end inside the contract crate: client → caller → dispatch → ticket.

use std::cell::RefCell;

use lattice_state::{Synced, Value};
use serde::{Deserialize, Serialize};

use crate::{
    App, AppError, CallError, Caller, Ctx, Dispatch, ExitStatus, ProcRef, RawCallError, RawTicket,
};

/// A counter app: `add` returns the new total, refuses past a cap.
struct Counter;

#[derive(Debug, PartialEq, Serialize, Deserialize, thiserror::Error)]
#[serde(rename_all = "snake_case")]
enum CountError {
    #[error("over the cap")]
    OverCap { cap: u32 },
}

impl App for Counter {
    const ID: &'static str = "test.counter";
    type State = Value<u32>;
    type Params = ();
    type Error = CountError;

    fn new(_: &()) -> Self {
        Self
    }

    fn start(&mut self, _: &mut Ctx, _: &mut Value<u32>) -> Result<(), CountError> {
        Ok(())
    }

    fn on_output(&mut self, _: &mut Ctx, _: &mut Value<u32>, _: ProcRef, _: &[u8]) {}

    fn on_exit(&mut self, _: &mut Ctx, _: &mut Value<u32>, _: ProcRef, _: ExitStatus) {}

    fn on_gap(&mut self, _: &mut Ctx, _: &mut Value<u32>) {}
}

#[crate::interface(crate = "crate")]
trait CounterApi: App<State = Value<u32>, Params = (), Error = CountError> {
    /// Adds `req`; returns the total.
    fn add(&mut self, ctx: &mut Ctx, state: &mut Value<u32>, req: u32) -> Result<u32, CountError>;
    fn reset(&mut self, ctx: &mut Ctx, state: &mut Value<u32>, req: ()) -> Result<(), CountError>;
}

impl CounterApi for Counter {
    fn add(&mut self, _: &mut Ctx, state: &mut Value<u32>, req: u32) -> Result<u32, CountError> {
        let total = state.get().saturating_add(req);
        if total > 10 {
            return Err(CountError::OverCap { cap: 10 });
        }
        state.set(total);
        Ok(total)
    }

    fn reset(&mut self, _: &mut Ctx, state: &mut Value<u32>, (): ()) -> Result<(), CountError> {
        state.set(0);
        Ok(())
    }
}

/// Runs every call at once against one in-memory instance, as a host would.
struct Loopback {
    app: RefCell<(Counter, Value<u32>)>,
}

impl Caller for Loopback {
    fn call(&self, action: Vec<u8>) -> RawTicket {
        let (app, state) = &mut *self.app.borrow_mut();
        let res = CounterApiRunner::<Counter>::dispatch(app, &mut Ctx::new(), state, &action);
        match &res {
            Ok(_) => state.commit(),
            Err(_) => state.rollback(),
        }
        RawTicket::ready(res.map_err(RawCallError::Refused))
    }
}

#[test]
fn a_typed_call_round_trips_through_dispatch() {
    let client = CounterApiClient::new(Loopback {
        app: RefCell::new((Counter, Value::new(0))),
    });
    assert_eq!(client.add(4).try_take(), Some(Ok(4)));
    assert_eq!(client.add(5).try_take(), Some(Ok(9)));
    let refused = client.add(5).try_take();
    assert_eq!(
        refused,
        Some(Err(CallError::App(CountError::OverCap { cap: 10 })))
    );
    assert_eq!(client.reset(()).try_take(), Some(Ok(())));
    assert_eq!(
        client.add(1).try_take(),
        Some(Ok(1)),
        "rolled back, then reset"
    );
}

#[test]
fn actions_encode_by_declaration_order_and_bad_bytes_are_refused() {
    let add = lattice_state::encode(&CounterApiAction::Add(3)).unwrap();
    let reset = lattice_state::encode(&CounterApiAction::Reset(())).unwrap();
    assert_eq!((add.first(), reset.first()), (Some(&0), Some(&1)));
    let mut app = Counter;
    let mut state = Value::new(0);
    let got = CounterApiRunner::<Counter>::dispatch(&mut app, &mut Ctx::new(), &mut state, &[7]);
    assert_eq!(got, Err(AppError::platform(crate::BAD_ACTION)));
    let got =
        CounterApiRunner::<Counter>::dispatch(&mut app, &mut Ctx::new(), &mut state, &[0, 20]);
    assert_eq!(
        got.map_err(|e| e.code().to_owned()),
        Err("over_cap".to_owned())
    );
}

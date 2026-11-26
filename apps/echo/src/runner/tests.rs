use std::cell::RefCell;

use super::*;
use crate::api::{EchoApiAction, EchoApiClient, EchoApiRunner};
use lattice_app::{CallError, Caller, Dispatch, RawCallError, RawTicket};
use lattice_state::{Draft, Synced, decode, encode};

/// A runner driven as the host drives it: one commit per call, rollback on `Err`.
struct Host {
    echo: Echo,
    state: EchoState,
    /// Follows by deltas alone, as a deck does.
    replica: EchoState,
    /// Deltas committed, in order.
    deltas: usize,
}

impl Host {
    /// A new instance: `new`, `start`, then `resumed` (06-apps.md, Runner rule 4).
    fn new() -> Self {
        let mut host = Self {
            echo: Echo::new(&EchoParams::default()),
            state: EchoState::default(),
            replica: EchoState::default(),
            deltas: 0,
        };
        let started = host.step(|e, ctx, state| e.start(ctx, state));
        assert_eq!(started, Ok(()));
        host.step(|e, ctx, state| {
            e.resumed(ctx, state);
            Ok(())
        })
        .unwrap();
        host
    }

    fn step<R>(
        &mut self,
        f: impl FnOnce(&mut Echo, &mut Ctx, &mut EchoState) -> Result<R, EchoError>,
    ) -> Result<R, EchoError> {
        let mut ctx = Ctx::new();
        let res = f(&mut self.echo, &mut ctx, &mut self.state);
        assert!(ctx.take_effects().is_empty(), "echo records no effect");
        if res.is_err() {
            self.state.rollback();
            return res;
        }
        if self.state.has_draft() {
            let change = encode(&Draft(&self.state)).unwrap();
            self.replica.apply(decode(&change).unwrap()).unwrap();
            self.deltas += 1;
        }
        self.state.commit();
        res
    }

    fn send(&mut self, text: &str) -> Result<u64, EchoError> {
        self.step(|e, ctx, state| e.send(ctx, state, Message::new(text)))
    }

    fn clear(&mut self) -> Result<(), EchoError> {
        self.step(|e, ctx, state| e.clear(ctx, state, ()))
    }

    fn lines(&self) -> Vec<(u64, String)> {
        self.state
            .lines()
            .iter()
            .map(|l| (l.get().seq(), l.get().text().to_owned()))
            .collect()
    }
}

#[test]
fn start_and_resumed_record_nothing() {
    let host = Host::new();
    assert!(host.state.lines().is_empty());
    assert_eq!(host.deltas, 0, "a new instance commits no delta");
}

#[test]
fn send_returns_increasing_seqs_and_appends_lines() {
    let mut host = Host::new();
    assert_eq!(host.send("hello"), Ok(1));
    assert_eq!(host.send("world"), Ok(2));
    assert_eq!(
        host.lines(),
        vec![(1, "hello".to_owned()), (2, "world".to_owned())]
    );
    assert_eq!(host.deltas, 2);
    assert_eq!(host.replica, host.state, "replica follows");
}

#[test]
fn refused_text_rolls_back_with_no_delta() {
    let cases = [
        ("empty", String::new(), EchoError::Empty),
        ("blank", " \t\n".to_owned(), EchoError::Empty),
        (
            "too_long",
            "x".repeat(MAX_TEXT_BYTES + 1),
            EchoError::TooLong,
        ),
    ];
    for (name, text, want) in cases {
        let mut host = Host::new();
        host.send("kept").unwrap();
        let deltas = host.deltas;
        assert_eq!(host.send(&text), Err(want), "case={name}");
        assert_eq!(host.deltas, deltas, "case={name}");
        assert_eq!(host.lines(), vec![(1, "kept".to_owned())], "case={name}");
        assert_eq!(host.send("next"), Ok(2), "no seq hole: case={name}");
    }
}

#[test]
fn text_of_exactly_4_kib_is_accepted() {
    let mut host = Host::new();
    assert_eq!(host.send(&"x".repeat(MAX_TEXT_BYTES)), Ok(1));
}

#[test]
fn clear_empties_and_seqs_keep_counting() {
    let mut host = Host::new();
    host.send("a").unwrap();
    host.send("b").unwrap();
    host.clear().unwrap();
    assert!(host.lines().is_empty());
    assert_eq!(host.replica, host.state);
    assert_eq!(host.send("c"), Ok(3));
    assert_eq!(host.lines(), vec![(3, "c".to_owned())]);
}

#[test]
fn clear_of_an_empty_list_commits_no_delta() {
    let mut host = Host::new();
    host.clear().unwrap();
    assert_eq!(host.deltas, 0);
}

#[test]
fn cap_keeps_at_most_1000_lines_newest_last() {
    let mut host = Host::new();
    for i in 1..=LINES_CAP {
        host.send(&i.to_string()).unwrap();
    }
    assert_eq!(host.state.lines().len(), LINES_CAP);
    let seq = host.send("overflow").unwrap();
    let lines = host.lines();
    assert_eq!(lines.len(), LINES_CAP / 2);
    assert_eq!(lines.last(), Some(&(seq, "overflow".to_owned())));
    let first = u64::try_from(LINES_CAP / 2 + 2).unwrap();
    assert_eq!(lines.first().map(|l| l.0), Some(first));
    assert!(lines.windows(2).all(|w| w[0].0 + 1 == w[1].0), "contiguous");
    assert_eq!(host.replica, host.state, "replica follows the rebuild");
    for i in 0..LINES_CAP * 2 {
        host.send(&i.to_string()).unwrap();
        assert!(host.state.lines().len() <= LINES_CAP);
    }
}

#[test]
fn restore_from_an_empty_journal_is_an_empty_state() {
    let mut host = Host::new();
    host.send("lost on restart").unwrap();
    // Restore (06-apps.md, Restore): `new`, `start` with effects dropped, no journal
    // entry (actions are never stored), then `resumed`.
    let restored = Host::new();
    assert!(restored.state.lines().is_empty());
    assert_eq!(
        encode(&restored.state).unwrap(),
        encode(&EchoState::default()).unwrap()
    );
    let mut restored = restored;
    assert_eq!(restored.send("again"), Ok(1), "seqs start over");
}

/// Runs every call at once against one in-memory instance, as the worker host would.
struct Loopback {
    inst: RefCell<(Echo, EchoState)>,
}

impl Caller for Loopback {
    fn call(&self, action: Vec<u8>) -> RawTicket {
        let (echo, state) = &mut *self.inst.borrow_mut();
        let res = EchoApiRunner::<Echo>::dispatch(echo, &mut Ctx::new(), state, &action);
        match &res {
            Ok(_) => state.commit(),
            Err(_) => state.rollback(),
        }
        RawTicket::ready(res.map_err(RawCallError::Refused))
    }
}

#[test]
fn client_round_trips_through_the_runner_adapter() {
    let client = EchoApiClient::new(Loopback {
        inst: RefCell::new((Echo::new(&EchoParams::default()), EchoState::default())),
    });
    assert_eq!(client.send(Message::new("hi")).try_take(), Some(Ok(1)));
    assert_eq!(client.send(Message::new("again")).try_take(), Some(Ok(2)));
    assert_eq!(
        client.send(Message::new("")).try_take(),
        Some(Err(CallError::App(EchoError::Empty)))
    );
    assert_eq!(client.clear(()).try_take(), Some(Ok(())));
    assert_eq!(client.send(Message::new("x")).try_take(), Some(Ok(3)));
}

#[test]
fn actions_encode_by_declaration_order_and_errors_by_name() {
    let send = encode(&EchoApiAction::Send(Message::new("a"))).unwrap();
    let clear = encode(&EchoApiAction::Clear(())).unwrap();
    assert_eq!((send.first(), clear.first()), (Some(&0), Some(&1)));
    let mut echo = Echo::new(&EchoParams::default());
    let mut state = EchoState::default();
    let action = encode(&EchoApiAction::Send(Message::new(" "))).unwrap();
    let err = EchoApiRunner::<Echo>::dispatch(&mut echo, &mut Ctx::new(), &mut state, &action)
        .unwrap_err();
    assert_eq!(err.code(), "empty");
    let too_long = lattice_app::AppError::of(&EchoError::TooLong);
    assert_eq!(too_long.code(), "too_long");
}

#[test]
fn params_encode_to_nothing() {
    assert!(encode(&EchoParams::default()).unwrap().is_empty());
    let _: EchoParams = decode(&[]).unwrap();
}

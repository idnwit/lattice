//! Tests of the runner core (`lattice_app::hosted`) with the host test app.

use lattice_app::{AppError, Effect, ExitStatus, Millis, ProcRef};

use super::{Input, Mode, hosted};
use crate::test_app::EchoRunner as Echo;

#[test]
fn replay_commits_deltas_but_drops_effects() {
    let (mut runner, start) = hosted::<Echo>(&[], Mode::Replay).unwrap();
    assert!(start.effects.is_empty(), "start spawns nothing on replay");
    let step = runner.step(Input::Output(ProcRef::from_raw(1), b"abc"), Mode::Replay);
    assert!(step.delta.is_some());
    let step = runner.step(
        Input::Exit(ProcRef::from_raw(1), ExitStatus::Code(0)),
        Mode::Replay,
    );
    assert!(step.effects.is_empty(), "finish is dropped on replay");
    let step = runner.step(Input::Wake(Millis(0)), Mode::Live);
    assert!(step.delta.is_none());
    assert!(!runner.snapshot().is_empty());
}

#[test]
fn a_refused_action_rolls_back() {
    let (mut runner, _) = hosted::<Echo>(&[], Mode::Live).unwrap();
    let before = runner.snapshot();
    // `echo` (tag 0) of a one-byte `Vec<u8>`: Echo refuses a non-empty request.
    let step = runner.step(Input::Action(&[0, 1, 7]), Mode::Live);
    assert!(step.delta.is_none());
    let refused = step.reply.and_then(Result::err);
    assert_eq!(refused.as_ref().map(AppError::code), Some("refused"));
    assert!(refused.is_some_and(|e| !e.is_platform()));
    assert_eq!(runner.snapshot(), before, "the draft rolled back");
    assert!(runner.step(Input::Gap, Mode::Live).effects.is_empty());
}

#[test]
fn an_action_that_does_not_decode_is_refused() {
    let (mut runner, _) = hosted::<Echo>(&[], Mode::Live).unwrap();
    let step = runner.step(Input::Action(&[0, 1]), Mode::Live);
    let want = AppError::platform(lattice_app::BAD_ACTION);
    assert_eq!(step.reply, Some(Err(want)));
    assert!(step.delta.is_none());
}

#[test]
fn live_start_runs_its_effects_and_an_accepted_action_commits() {
    let (mut runner, start) = hosted::<Echo>(&[], Mode::Live).unwrap();
    assert_eq!(
        start.effects,
        vec![Effect::Spawn {
            template: "main".to_owned(),
            cwd: String::new(),
            size: None,
        }]
    );
    let step = runner.step(Input::Action(&[0, 0]), Mode::Live);
    let max = lattice_state::encode(&u64::MAX).unwrap();
    assert_eq!(
        step.reply,
        Some(Ok(max.clone())),
        "the reply is the encoded `Res`"
    );
    assert_eq!(step.delta, Some(max));
}

#[test]
fn params_that_do_not_decode_build_no_runner() {
    assert!(hosted::<Echo>(&[1], Mode::Live).is_none());
}

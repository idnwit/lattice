//! `lattice probe` and `lattice probe-echo`: experiment clients (M1).
//!
//! `probe` opens a `terminal` instance, resizes it once, types a command, and prints the
//! screen. `probe-echo` sends one line to an `echo` instance and prints the lines.

use std::time::Duration;

use lattice_app::{AppId, InstanceId, InstancePhase, InstanceRecord, Ticket};
use lattice_app_echo::{EchoApiClient, EchoParams, EchoState, Message};
use lattice_app_terminal::{
    Input, Resize, Size, TermError, TermParams, TermState, TerminalApiClient,
};
use lattice_deck_core::{Replica, StateUpdate};

/// Columns of the probe terminal.
const PROBE_COLS: u16 = 100;
/// Rows of the probe terminal.
const PROBE_ROWS: u16 = 30;

pub(crate) async fn probe(
    worker: Option<&str>,
    command: &str,
    exit: bool,
    attach: Option<u64>,
) -> anyhow::Result<()> {
    let mut link = super::open_link(worker).await?;

    let params = lattice_state::encode(&new_params())?;
    let app = AppId::try_new(lattice_app_terminal::APP_ID)?;
    let iid = match attach {
        Some(raw) => lattice_app::InstanceId::from_raw(raw),
        None => {
            let package = super::builtin(&app)?;
            link.create(app, package, params, "probe".to_owned())
                .await?
        }
    };
    println!("instance: {iid}");
    let state = format!("inst/{iid}/state");
    link.subscribe(state.parse()?).await?;
    link.subscribe("worker/apps".parse()?).await?;

    let client = TerminalApiClient::new(link.caller(iid));
    let mut resized = Some(client.resize(Resize::new(PROBE_COLS, PROBE_ROWS)));
    link.flush_calls().await?;

    let mut replica = Replica::<TermState>::default();
    let mut typed = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(if exit { 6 } else { 3 });
    while let Ok(msg) = tokio::time::timeout_at(deadline, link.next_message()).await {
        let msg = msg?;
        report_resize(&mut resized);
        let Some((stream, update)) = StateUpdate::from_message(msg, &link.worker()) else {
            continue;
        };
        let stream: String = stream.into();
        if stream != state {
            anyhow::ensure!(!failed_in(&update, iid), "instance.failed (instance {iid})");
            continue;
        }
        update.apply_to(&mut replica);
        // The runner binds the shell on its first output; type only after the prompt shows.
        if !typed && has_prompt(replica.state()) {
            typed = true;
            // Not awaited: the shell's echo arrives as state.
            drop(client.input(Input::new(typed_line(command, exit))));
            link.flush_calls().await?;
        }
    }
    print_screen(replica.state(), replica.version());
    Ok(())
}

pub(crate) async fn probe_echo(worker: Option<&str>, text: &str) -> anyhow::Result<()> {
    let mut link = super::open_link(worker).await?;
    let params = lattice_state::encode(&EchoParams::default())?;
    let app = AppId::try_new(lattice_app_echo::APP_ID)?;
    let package = super::builtin(&app)?;
    let iid = link
        .create(app, package, params, "probe".to_owned())
        .await?;
    println!("instance: {iid}");
    link.subscribe(format!("inst/{iid}/state").parse()?).await?;

    let client = EchoApiClient::new(link.caller(iid));
    let mut ticket = client.send(Message::new(text));
    link.flush_calls().await?;

    let mut replica = Replica::<EchoState>::default();
    let mut seq = None;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    while let Ok(msg) = tokio::time::timeout_at(deadline, link.next_message()).await {
        let msg = msg?;
        if let Some(outcome) = ticket.try_take() {
            let got = outcome.map_err(|e| anyhow::anyhow!("send failed: {e}"))?;
            println!("send: seq {got}");
            seq = Some(got);
        }
        if let Some((_, update)) = StateUpdate::from_message(msg, &link.worker()) {
            update.apply_to(&mut replica);
        }
        let shown = |s| replica.state().lines().iter().any(|l| l.get().seq() == s);
        if seq.is_some_and(shown) {
            break;
        }
    }
    println!("--- lines (version {:?}) ---", replica.version());
    for line in replica.state().lines() {
        println!("{} {}", line.get().seq(), line.get().text());
    }
    Ok(())
}

/// Params of a new probe instance (specs/03-sessions.md, Probe rule 1).
/// Empty cwd: the worker picks its default. A deck path may not exist there.
fn new_params() -> TermParams {
    TermParams::default().with_size(PROBE_COLS, PROBE_ROWS)
}

/// True when a full `worker/apps` state lists `iid` as `failed` (Probe rule 2).
/// `worker/apps` syncs full lists only (06-apps.md, Instances rule 4).
fn failed_in(update: &StateUpdate, iid: InstanceId) -> bool {
    let StateUpdate::Full { bytes, .. } = update else {
        return false;
    };
    InstanceRecord::decode_list(bytes)
        .unwrap_or_default()
        .iter()
        .any(|r| r.iid() == iid && r.phase() == InstancePhase::Failed)
}

/// Prints the outcome of the probe's `resize` once it settles.
fn report_resize(ticket: &mut Option<Ticket<Size, TermError>>) {
    if let Some(outcome) = ticket.as_mut().and_then(Ticket::try_take) {
        *ticket = None;
        match outcome {
            Ok(size) => println!("resize: {}x{}", size.cols(), size.rows()),
            Err(e) => println!("resize failed: {e}"),
        }
    }
}

/// True once any screen row shows text.
fn has_prompt(state: &TermState) -> bool {
    state
        .screen()
        .iter()
        .any(|r| !r.get().text().trim().is_empty())
}

/// `command`, Enter, and `exit` + Enter when asked.
fn typed_line(command: &str, exit: bool) -> Vec<u8> {
    let mut line = command.as_bytes().to_vec();
    line.extend_from_slice(b"\r");
    if exit {
        line.extend_from_slice(b"exit\r");
    }
    line
}

fn print_screen(state: &TermState, version: Option<u64>) {
    println!(
        "--- screen (version {version:?}, title {:?}) ---",
        state.title()
    );
    let rows: Vec<String> = state.screen().iter().map(|r| r.get().text()).collect();
    let last = rows.iter().rposition(|r| !r.trim().is_empty()).unwrap_or(0);
    for row in rows.iter().take(last.saturating_add(1)) {
        println!("{row}");
    }
    println!(
        "--- scrollback rows: {} ---",
        state.scrollback().iter().count()
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use lattice_app::JournalMeta;

    #[test]
    fn a_new_probe_instance_starts_in_the_worker_default_cwd() {
        let want = TermParams::default().with_size(PROBE_COLS, PROBE_ROWS);
        assert_eq!(new_params(), want, "no deck-local path is sent");
    }

    fn apps(phase: InstancePhase) -> StateUpdate {
        let app = AppId::try_new(lattice_app_terminal::APP_ID).unwrap();
        let meta = JournalMeta::new(app, None, Vec::new(), "probe".to_owned());
        let mut record = InstanceRecord::new(InstanceId::from_raw(4), &meta);
        record.set_phase(phase);
        StateUpdate::Full {
            version: 1,
            bytes: InstanceRecord::encode_list(&[record]),
        }
    }

    #[test]
    fn a_failed_probe_instance_is_seen_in_worker_apps() {
        let iid = InstanceId::from_raw(4);
        assert!(failed_in(&apps(InstancePhase::Failed), iid));
        assert!(!failed_in(&apps(InstancePhase::Running), iid));
        assert!(!failed_in(
            &apps(InstancePhase::Failed),
            InstanceId::from_raw(5)
        ));
    }
}

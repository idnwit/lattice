//! ROADMAP M1.9 and M1.4c: `latticed` restarts mid-session; the instance survives.
//!
//! Real processes on 127.0.0.1: `latticed-proc`, then `latticed` twice, and an in-process deck
//! (`lattice-deck-core` over QUIC, as `lattice probe` runs it). The deck keeps the version it
//! held before the restart and resumes with it (06-apps.md, Sync to viewers rules 3–5, 10–11).
//!
//! The daemons are built with `cargo build` first: a test only gets its own package's binaries.

// Inside a `cfg(test)` module so the helpers get the test lint allowances (clippy.toml).
#[cfg(test)]
mod restart {
    use std::net::SocketAddr;
    use std::path::{Path, PathBuf};
    use std::process::{Child, Command, Stdio};
    use std::time::Duration;

    use lattice_app::{AppId, InstanceId, InstancePhase, InstanceRecord, Replica};
    use lattice_app_terminal::{Input, TermParams, TermState, TerminalApiClient};
    use lattice_deck_core::{DeckHost, StateUpdate, WorkerLink};
    use lattice_key::{NodeId, NodeKey, SecretKeyBytes};
    use lattice_quic::{QuicConfig, QuicConn, QuicTransport};
    use tokio::time::{Instant, timeout_at};

    /// Upper bound for one step (a debug build compiles the terminal runner at boot).
    const STEP: Duration = Duration::from_secs(60);
    /// Typed before the restart; the shell prints `one-42`.
    const FIRST: &[u8] = b"echo one-$((40+2))\r";
    /// Typed after the restart; the shell prints `two-42`.
    const SECOND: &[u8] = b"echo two-$((40+2))\r";

    /// Child daemons, killed on drop so a failed test leaves none behind.
    struct Daemons {
        dir: PathBuf,
        proc: Child,
        worker: Option<Child>,
        bins: Bins,
    }

    struct Bins {
        latticed: PathBuf,
        proc: PathBuf,
    }

    impl Drop for Daemons {
        fn drop(&mut self) {
            for child in self.worker.iter_mut().chain([&mut self.proc]) {
                drop(child.kill());
                drop(child.wait());
            }
            drop(std::fs::remove_dir_all(&self.dir));
        }
    }

    /// Builds `latticed` and `latticed-proc` next to the `lattice` test binary.
    fn build_daemons() -> Bins {
        let exe = Path::new(env!("CARGO_BIN_EXE_lattice"));
        let out = exe.parent().unwrap();
        let mut cmd = Command::new(env!("CARGO"));
        // `--locked` as in the gate. No feature flags: neither daemon declares features.
        cmd.args([
            "build",
            "--quiet",
            "--locked",
            "-p",
            "latticed",
            "-p",
            "latticed-proc",
        ])
        .arg("--target-dir")
        .arg(out.parent().unwrap());
        if out.ends_with("release") {
            cmd.arg("--release");
        }
        assert!(
            cmd.status().unwrap().success(),
            "cargo build of the daemons"
        );
        Bins {
            latticed: out.join("latticed"),
            proc: out.join("latticed-proc"),
        }
    }

    fn deck_key() -> NodeKey {
        NodeKey::from_secret(SecretKeyBytes::new([7; 32]))
    }

    /// Starts `latticed-proc` in a fresh scratch dir; the deck is in the worker's allowlist.
    fn start_proc() -> Daemons {
        let bins = build_daemons();
        let dir = std::env::temp_dir().join(format!("lt-restart-{}", std::process::id()));
        drop(std::fs::remove_dir_all(&dir));
        std::fs::create_dir_all(dir.join("data/worker")).unwrap();
        std::fs::create_dir_all(dir.join("tmp")).unwrap();
        let allow = format!("{} test-deck\n", deck_key().node_id());
        std::fs::write(dir.join("data/worker/decks"), allow).unwrap();
        let proc = env(&mut Command::new(&bins.proc), &dir).spawn().unwrap();
        Daemons {
            dir,
            proc,
            worker: None,
            bins,
        }
    }

    /// The isolated environment of both daemons.
    fn env<'a>(cmd: &'a mut Command, dir: &Path) -> &'a mut Command {
        cmd.env("LATTICE_PROC_SOCK", dir.join("p.sock"))
            .env("LATTICE_DATA_DIR", dir.join("data"))
            .env("TMPDIR", dir.join("tmp"))
            .env("HOME", dir)
            .env("SHELL", "/bin/sh")
            // The shell outlives the dir by a moment on hang-up: no history file to write.
            .env("HISTFILE", "/dev/null")
            .stdout(Stdio::null())
    }

    impl Daemons {
        /// (Re)starts `latticed`; returns its NodeId and address from `worker.addr`.
        async fn start_worker(&mut self) -> (NodeId, SocketAddr) {
            let card = self.dir.join("tmp/lattice/worker.addr");
            drop(std::fs::remove_file(&card));
            let mut cmd = Command::new(&self.bins.latticed);
            env(&mut cmd, &self.dir).args(["--listen", "127.0.0.1:0", "--proc-sock"]);
            self.worker = Some(cmd.arg(self.dir.join("p.sock")).spawn().unwrap());
            let deadline = Instant::now() + STEP;
            loop {
                if let Ok(text) = std::fs::read_to_string(&card)
                    && let Some((id, addr)) = text.trim().split_once('@')
                {
                    return (id.parse().unwrap(), addr.parse().unwrap());
                }
                assert!(Instant::now() < deadline, "latticed did not start");
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }

        /// Kills `latticed` only; `latticed-proc` and the shell keep running.
        fn kill_worker(&mut self) {
            let mut worker = self.worker.take().unwrap();
            worker.kill().unwrap();
            worker.wait().unwrap();
        }
    }

    async fn open_link(worker: NodeId, addr: SocketAddr) -> WorkerLink<QuicConn> {
        let bind: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let transport = QuicTransport::bind(QuicConfig::direct_lan(deck_key(), bind))
            .await
            .unwrap();
        transport.add_peer(worker, addr);
        tokio::time::timeout(STEP, DeckHost::new(transport).open_link(worker))
            .await
            .unwrap()
            .unwrap()
    }

    /// What the deck holds: the instance state and the `worker/apps` records.
    #[derive(Default)]
    struct Deck {
        state: Replica<TermState>,
        records: Vec<InstanceRecord>,
        /// Kind of each state update, in order: `true` for a full state.
        updates: Vec<bool>,
    }

    impl Deck {
        /// Reads messages until `done` holds; panics after [`STEP`].
        async fn pump(&mut self, link: &mut WorkerLink<QuicConn>, done: impl Fn(&Self) -> bool) {
            let deadline = Instant::now() + STEP;
            let apps = "worker/apps".parse().unwrap();
            while !done(self) {
                let msg = timeout_at(deadline, link.next_message()).await;
                let msg = msg.expect("deck waited too long").unwrap();
                let Some((stream, update)) = StateUpdate::from_message(msg, &link.worker()) else {
                    continue;
                };
                if stream == apps {
                    if let StateUpdate::Full { bytes, .. } = update {
                        self.records = InstanceRecord::decode_list(&bytes).unwrap();
                    }
                    continue;
                }
                self.updates
                    .push(matches!(update, StateUpdate::Full { .. }));
                update.apply_to(&mut self.state);
            }
        }

        /// Every row, scrollback first, trimmed.
        fn rows(&self) -> Vec<String> {
            let s = self.state.state();
            let rows = s.scrollback().iter().chain(s.screen().iter());
            rows.map(|r| r.get().text().trim_end().to_owned()).collect()
        }

        fn count(&self, line: &str) -> usize {
            self.rows().iter().filter(|r| *r == line).count()
        }

        /// True once `line` shows and the next prompt follows it: the shell is idle.
        fn idle_after(&self, line: &str) -> bool {
            let rows = self.rows();
            let mut after = rows.iter().skip_while(|r| *r != line).skip(1);
            after.any(|r| !r.is_empty())
        }

        fn phase(&self, iid: InstanceId) -> Option<InstancePhase> {
            let record = self.records.iter().find(|r| r.iid() == iid)?;
            Some(record.phase())
        }
    }

    async fn subscribe(link: &mut WorkerLink<QuicConn>, iid: InstanceId, have: Option<u64>) {
        link.subscribe("worker/apps".parse().unwrap())
            .await
            .unwrap();
        let stream = format!("inst/{iid}/state").parse().unwrap();
        link.subscribe_at(stream, have).await.unwrap();
    }

    async fn type_line(link: &mut WorkerLink<QuicConn>, iid: InstanceId, line: &[u8]) {
        let client = TerminalApiClient::new(link.caller(iid));
        // Not awaited: the shell's echo arrives as state.
        drop(client.input(Input::new(line.to_vec())));
        link.flush_calls().await.unwrap();
    }

    /// Boot 1: creates a terminal, runs one command, returns what the deck holds.
    async fn first_boot(link: &mut WorkerLink<QuicConn>, dir: &Path) -> (InstanceId, Deck) {
        let app = AppId::try_new(lattice_app_terminal::APP_ID).unwrap();
        let builtin = lattice_builtin_apps::by_id(app.as_str()).unwrap();
        let cwd = dir.to_string_lossy().into_owned();
        let params = lattice_state::encode(&TermParams::new(cwd).with_size(80, 24)).unwrap();
        let package = (builtin.package(), builtin.bytes);
        let iid = link.create(app, package, params, "restart".to_owned());
        let iid = tokio::time::timeout(STEP, iid).await.unwrap().unwrap();
        let mut deck = Deck::default();
        subscribe(link, iid, None).await;
        // The runner binds the shell on its first output; type only after the prompt shows.
        deck.pump(link, |d| d.rows().iter().any(|r| !r.is_empty()))
            .await;
        type_line(link, iid, FIRST).await;
        deck.pump(link, |d| d.idle_after("one-42")).await;
        (iid, deck)
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn an_instance_survives_a_latticed_restart() {
        let mut daemons = start_proc();
        let (worker, addr) = daemons.start_worker().await;
        let mut link = open_link(worker, addr).await;
        let (iid, mut after) = first_boot(&mut link, &daemons.dir).await;
        let held = after.state.version().unwrap();
        let rows_before = after.rows();

        daemons.kill_worker();
        drop(link);
        let (again, addr) = daemons.start_worker().await;
        assert_eq!(again, worker, "same node key after the restart");
        let mut link = open_link(worker, addr).await;

        // The deck resumes with its pre-restart version: it gets the full state, never deltas.
        after.updates.clear();
        after.records.clear();
        subscribe(&mut link, iid, Some(held)).await;
        after
            .pump(&mut link, |d| {
                !d.updates.is_empty() && !d.records.is_empty()
            })
            .await;
        assert_eq!(
            after.updates.first(),
            Some(&true),
            "first update is a full state"
        );
        let version = after.state.version().unwrap();
        assert!(version > held, "version={version} held={held}");

        // Same iid, same screen; replay wrote nothing to the PTY, so the output shows once.
        assert_eq!(after.phase(iid), Some(InstancePhase::Running), "no gap");
        assert_eq!(after.rows(), rows_before, "the screen survives");
        assert_eq!(after.count("one-42"), 1);

        // The shell is the same process and stays live: output after the restart arrives too.
        type_line(&mut link, iid, SECOND).await;
        after.pump(&mut link, |d| d.count("two-42") == 1).await;
        assert_eq!(after.count("one-42"), 1, "output is complete");
    }
}

//! `lattice config` end to end (specs/10-settings.md, rules 34, 41–46; specs/11, rules 1, 13).
//!
//! Real processes: the `lattice` CLI, and for the daemon cases `latticed-proc` and
//! `latticed` on 127.0.0.1, each in its own scratch data dir.

// Inside a `cfg(test)` module so the helpers get the test lint allowances (clippy.toml).
#[cfg(test)]
mod config {
    use std::path::{Path, PathBuf};
    use std::process::{Child, Command, Output, Stdio};
    use std::time::{Duration, Instant};

    use lattice_control::DirLock;

    /// Upper bound for a daemon start (a debug build).
    const START: Duration = Duration::from_secs(60);

    /// A scratch dir, removed on drop with any daemons in it.
    struct Scratch {
        dir: PathBuf,
        children: Vec<Child>,
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            for child in &mut self.children {
                drop(child.kill());
                drop(child.wait());
            }
            drop(std::fs::remove_dir_all(&self.dir));
        }
    }

    impl Scratch {
        /// A short name: the control socket path has a length limit.
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("ltcfg-{name}-{}", std::process::id()));
            drop(std::fs::remove_dir_all(&dir));
            std::fs::create_dir_all(dir.join("tmp")).unwrap();
            Self {
                dir,
                children: Vec::new(),
            }
        }

        fn worker(&self) -> PathBuf {
            self.dir.join("data/worker")
        }

        fn file(&self) -> PathBuf {
            self.worker().join("config.toml")
        }

        /// The isolated environment of every process.
        fn env<'a>(&self, cmd: &'a mut Command) -> &'a mut Command {
            cmd.env("LATTICE_PROC_SOCK", self.dir.join("p.sock"))
                .env("LATTICE_DATA_DIR", self.dir.join("data"))
                .env("TMPDIR", self.dir.join("tmp"))
                .env_remove("XDG_RUNTIME_DIR")
                .env_remove("LATTICE_CONFIG")
                .env_remove("LATTICE_RELAY")
                .env("HOME", &self.dir)
        }

        /// Runs `lattice config <args>`.
        fn lattice(&self, args: &[&str]) -> Output {
            let mut cmd = Command::new(env!("CARGO_BIN_EXE_lattice"));
            self.env(&mut cmd).arg("config").args(args);
            cmd.output().unwrap()
        }

        /// Starts `latticed-proc` and `latticed`; returns once the control socket answers.
        fn start_daemons(&mut self) {
            let bins = build_daemons();
            let mut proc = Command::new(&bins.0);
            self.env(&mut proc).stdout(Stdio::null());
            self.children.push(proc.spawn().unwrap());
            let mut latticed = Command::new(&bins.1);
            self.env(&mut latticed)
                .args(["--listen", "127.0.0.1:0", "--proc-sock"])
                .arg(self.dir.join("p.sock"))
                .stdout(Stdio::null());
            self.children.push(latticed.spawn().unwrap());
            let deadline = Instant::now() + START;
            loop {
                let out = self.lattice(&["list"]);
                if out.status.success() && !stdout(&out).contains("not running") {
                    return;
                }
                assert!(Instant::now() < deadline, "latticed did not start");
                std::thread::sleep(Duration::from_millis(100));
            }
        }
    }

    fn stdout(out: &Output) -> String {
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    fn stderr(out: &Output) -> String {
        String::from_utf8_lossy(&out.stderr).into_owned()
    }

    /// Builds `latticed-proc` and `latticed` next to the `lattice` test binary.
    fn build_daemons() -> (PathBuf, PathBuf) {
        let exe = Path::new(env!("CARGO_BIN_EXE_lattice"));
        let out = exe.parent().unwrap();
        let mut cmd = Command::new(env!("CARGO"));
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
        (out.join("latticed-proc"), out.join("latticed"))
    }

    /// Rules 34, 41–43, 46 without a daemon.
    #[test]
    fn without_a_daemon_the_cli_writes_the_file_under_the_lock() {
        let s = Scratch::new("nod");
        let out = s.lattice(&["set", "relay.addr", "udp://relay.example.com:7400"]);
        assert!(out.status.success(), "{}", stderr(&out));
        assert!(stdout(&out).contains("relay changed: paired decks must pair again"));
        let text = std::fs::read_to_string(s.file()).unwrap();
        assert_eq!(text, "[relay]\naddr = \"udp://relay.example.com:7400\"\n");

        let out = s.lattice(&["get", "relay.addr"]);
        assert_eq!(
            stdout(&out),
            "latticed not running: flags not shown\nudp://relay.example.com:7400\n"
        );

        for (args, code) in [
            (
                &["set", "relay.addr", "udp://Relay.example.com:7400"][..],
                "config.invalid_value",
            ),
            (&["set", "colour", "red"][..], "config.unknown_key"),
            (&["get", "colour"][..], "config.unknown_key"),
        ] {
            let out = s.lattice(args);
            assert!(!out.status.success(), "case={args:?}");
            assert!(
                stderr(&out).contains(code),
                "case={args:?} stderr={}",
                stderr(&out)
            );
        }
        assert_eq!(
            std::fs::read_to_string(s.file()).unwrap(),
            text,
            "file unchanged"
        );

        let out = s.lattice(&["reset", "relay.addr"]);
        assert!(out.status.success(), "{}", stderr(&out));
        assert_eq!(std::fs::read_to_string(s.file()).unwrap(), "");
    }

    /// Rule 34: the lock is taken and nothing answers: `worker.busy`, never a write.
    #[test]
    fn a_held_lock_without_a_socket_is_busy() {
        let s = Scratch::new("busy");
        let uid = rustix::process::getuid().as_raw();
        let _starting = DirLock::acquire(&s.worker(), uid).unwrap();
        let out = s.lattice(&["set", "name", "w1"]);
        assert!(!out.status.success());
        assert!(stderr(&out).contains("worker.busy"), "{}", stderr(&out));
        assert!(!s.file().exists());
    }

    /// Rule 45 without a daemon: a file changed during the edit is a conflict.
    #[test]
    fn edit_replaces_the_file_only_when_it_did_not_change() {
        let s = Scratch::new("edit");
        std::fs::create_dir_all(s.worker()).unwrap();
        std::fs::write(s.file(), "name = \"a\"\n").unwrap();
        // The "editor" rewrites the copy; the second one also changes the real file.
        let ok = "printf 'name = \"b\"\\n' >";
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_lattice"));
        s.env(&mut cmd).env("EDITOR", ok).args(["config", "edit"]);
        let out = cmd.output().unwrap();
        assert!(out.status.success(), "{}", stderr(&out));
        assert_eq!(std::fs::read_to_string(s.file()).unwrap(), "name = \"b\"\n");

        let racy = format!(
            "printf 'name = \"r\"\\n' > {} ; printf 'name = \"c\"\\n' >",
            s.file().display()
        );
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_lattice"));
        s.env(&mut cmd).env("EDITOR", racy).args(["config", "edit"]);
        let out = cmd.output().unwrap();
        assert!(stderr(&out).contains("config.conflict"), "{}", stderr(&out));
        assert_eq!(std::fs::read_to_string(s.file()).unwrap(), "name = \"r\"\n");
    }

    /// Rules 34, 41, 43 with a daemon: the CLI goes through the socket, flags show.
    #[test]
    fn with_a_daemon_the_cli_goes_through_the_socket() {
        let mut s = Scratch::new("d");
        s.start_daemons();
        let out = s.lattice(&["get", "listen.addr"]);
        assert_eq!(
            stdout(&out),
            "127.0.0.1:0\n",
            "the flag value of the daemon"
        );
        let out = s.lattice(&["list"]);
        assert!(
            stdout(&out).contains("listen.addr       127.0.0.1:0 (flag)"),
            "{}",
            stdout(&out)
        );

        let out = s.lattice(&["set", "name", "w1"]);
        assert!(out.status.success(), "{}", stderr(&out));
        assert_eq!(stdout(&out), "name              w1 (file)\n");
        assert_eq!(
            std::fs::read_to_string(s.file()).unwrap(),
            "name = \"w1\"\n"
        );

        let out = s.lattice(&["set", "listen.addr", "nope"]);
        assert!(
            stderr(&out).contains("config.invalid_value"),
            "{}",
            stderr(&out)
        );

        // Rule 35: an edit on disk applies without a restart.
        std::fs::write(s.file(), "name = \"w2\"\n").unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        while stdout(&s.lattice(&["get", "name"])) != "w2\n" {
            assert!(Instant::now() < deadline, "reload");
            std::thread::sleep(Duration::from_millis(100));
        }

        // Specs/11 Invariant 5: a second latticed on the same data dir exits.
        let (_, latticed) = build_daemons();
        let mut second = Command::new(latticed);
        s.env(&mut second).args(["--listen", "127.0.0.1:0"]);
        let out = second.output().unwrap();
        assert!(!out.status.success());
        assert!(
            stderr(&out).contains("worker.already_running"),
            "{}",
            stderr(&out)
        );
    }
}

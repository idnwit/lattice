//! `worker/config.toml`, owned by `latticed` (specs/10-settings.md, Worker config).
//!
//! `latticed` is the only writer while it runs (rule 34): every write holds the store's
//! mutex and the `worker/` dir lock. The file is polled for changes (rule 35); a bad edit
//! keeps the last valid values (rule 36).

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use anyhow::Context;
use lattice_control::{
    ConfigError, ConfigFile, DirLock, Entry, Key, Layers, Reply, Request, Resolved, file_hash,
    read_or_empty,
};
use tokio::sync::watch;

/// How often the file is checked for changes. TBD: the spec sets no reload latency
/// (specs/10-settings.md, rule 35). Is 1 s right?
pub(crate) const CONFIG_POLL: Duration = Duration::from_secs(1);

/// `$LATTICE_CONFIG`, else `<worker dir>/config.toml` (rule 32).
pub(crate) fn config_path(worker: &Path) -> PathBuf {
    lattice_control::config_path(worker, std::env::var_os(lattice_control::CONFIG_ENV))
}

/// The config file, its resolution, and the dir lock that makes `latticed` its only writer.
#[derive(Debug)]
pub(crate) struct ConfigStore {
    path: PathBuf,
    layers: Layers,
    lock: DirLock,
    /// Hash of the file bytes last read: the watcher skips an unchanged file.
    seen: Mutex<[u8; 32]>,
    resolved: watch::Sender<Resolved>,
}

impl ConfigStore {
    /// Reads and resolves the file. A file that fails to parse or validate fails the
    /// start with its code (rule 36).
    pub(crate) fn load(path: PathBuf, layers: Layers, lock: DirLock) -> anyhow::Result<Self> {
        let bytes = read_or_empty(&path).with_context(|| format!("read {}", path.display()))?;
        let file = parse(&bytes).map_err(|e| anyhow::anyhow!("{e}: {}", path.display()))?;
        let resolved = layers.resolve(&file);
        Ok(Self {
            path,
            layers,
            lock,
            seen: Mutex::new(file_hash(&bytes)),
            resolved: watch::Sender::new(resolved),
        })
    }

    /// The resolved config now.
    pub(crate) fn current(&self) -> Resolved {
        self.resolved.borrow().clone()
    }

    /// Notified on each change of the resolved config.
    pub(crate) fn subscribe(&self) -> watch::Receiver<Resolved> {
        self.resolved.subscribe()
    }

    /// Answers one control request (specs/11-control-socket.md, Messages). Blocking: call
    /// it off the async workers.
    pub(crate) fn handle(&self, req: Request) -> Reply {
        let result = match req {
            Request::ConfigList => Ok(self.current().entries().to_vec()),
            Request::ConfigGet { key } => self.entry(&key),
            Request::ConfigSet { key, value } => self.edit(&key, |f, k| f.set(k, &value)),
            Request::ConfigReset { key } => self.edit(&key, |f, k| {
                f.reset(k);
                Ok(())
            }),
            Request::ConfigReplace { toml, base_hash } => self.replace(&toml, &base_hash),
            // Pairing over the socket lands with D53 (ROADMAP M2.9).
            Request::AddDeck { .. } => return Reply::failed("control.unsupported"),
        };
        match result {
            Ok(entries) => Reply::Config { entries },
            Err(e) => {
                tracing::info!(code = e.code(), "config request refused");
                Reply::failed(e.code())
            }
        }
    }

    fn entry(&self, key: &str) -> Result<Vec<Entry>, ConfigError> {
        let key: Key = key.parse()?;
        Ok(self.current().entry(key).cloned().into_iter().collect())
    }

    /// Reads the file, changes `key`, writes it back, and applies it.
    fn edit(
        &self,
        key: &str,
        change: impl FnOnce(&mut ConfigFile, Key) -> Result<(), ConfigError>,
    ) -> Result<Vec<Entry>, ConfigError> {
        let key: Key = key.parse()?;
        let mut seen = lock(&self.seen);
        let mut file = parse(&self.read()?)?;
        change(&mut file, key)?;
        let text = file.to_toml();
        self.write(&mut seen, text.as_bytes())?;
        self.apply(&file);
        Ok(self.current().entry(key).cloned().into_iter().collect())
    }

    /// `ConfigReplace`: only when the file still hashes to `base_hash` and `toml` is valid.
    fn replace(&self, toml: &str, base_hash: &[u8; 32]) -> Result<Vec<Entry>, ConfigError> {
        let mut seen = lock(&self.seen);
        if file_hash(&self.read()?) != *base_hash {
            return Err(ConfigError::Conflict);
        }
        let file = ConfigFile::parse(toml)?;
        self.write(&mut seen, toml.as_bytes())?;
        self.apply(&file);
        Ok(self.current().entries().to_vec())
    }

    /// Re-reads the file when it changed (rule 35). A bad file keeps the last valid
    /// values and logs its code (rule 36).
    pub(crate) fn reload(&self) {
        let mut seen = lock(&self.seen);
        let bytes = match self.read() {
            Ok(bytes) => bytes,
            Err(e) => {
                tracing::warn!(code = e.code(), path = %self.path.display(), "config not read");
                return;
            }
        };
        let hash = file_hash(&bytes);
        if hash == *seen {
            return;
        }
        *seen = hash;
        match parse(&bytes) {
            Ok(file) => {
                tracing::info!(path = %self.path.display(), "config reloaded");
                self.apply(&file);
            }
            Err(e) => tracing::warn!(
                code = e.code(),
                error = %e,
                "config change refused: keeping the last valid values"
            ),
        }
    }

    fn read(&self) -> Result<Vec<u8>, ConfigError> {
        read_or_empty(&self.path).map_err(|e| ConfigError::Io(e.to_string()))
    }

    fn write(&self, seen: &mut [u8; 32], bytes: &[u8]) -> Result<(), ConfigError> {
        self.lock
            .write_atomic(&self.path, bytes)
            .map_err(|e| ConfigError::Io(e.to_string()))?;
        *seen = file_hash(bytes);
        Ok(())
    }

    fn apply(&self, file: &ConfigFile) {
        let next = self.layers.resolve(file);
        self.resolved.send_if_modified(|now| {
            let changed = *now != next;
            *now = next;
            changed
        });
    }
}

/// Parses file bytes; they must be UTF-8 TOML.
fn parse(bytes: &[u8]) -> Result<ConfigFile, ConfigError> {
    let text = std::str::from_utf8(bytes).map_err(|e| ConfigError::Parse(e.to_string()))?;
    ConfigFile::parse(text)
}

/// Locks `m`. Poisoning is ignored: the guarded hash is always a whole value.
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lattice_control::Source;

    fn store(name: &str) -> (PathBuf, ConfigStore) {
        let root = std::env::temp_dir().join(format!("latticed-cfg-{name}-{}", std::process::id()));
        drop(std::fs::remove_dir_all(&root));
        let worker = root.join("worker");
        let lock = DirLock::acquire(&worker, crate::runtime::uid()).unwrap();
        let store = ConfigStore::load(worker.join("config.toml"), Layers::new("h".into()), lock);
        (root, store.unwrap())
    }

    fn entries(reply: Reply) -> Vec<Entry> {
        match reply {
            Reply::Config { entries } => entries,
            Reply::Failed { code } => panic!("failed: {code}"),
        }
    }

    #[test]
    fn set_writes_the_file_and_applies_live() {
        let (root, store) = store("set");
        let mut rx = store.subscribe();
        let reply = store.handle(Request::ConfigSet {
            key: "relay.addr".into(),
            value: "udp://relay.example.com:7400".into(),
        });
        let got = entries(reply);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].source(), Source::File);
        assert!(rx.has_changed().unwrap(), "watchers see the change");
        let text = std::fs::read_to_string(root.join("worker/config.toml")).unwrap();
        assert_eq!(text, "[relay]\naddr = \"udp://relay.example.com:7400\"\n");
        rx.mark_unchanged();
        let reply = store.handle(Request::ConfigReset {
            key: "relay.addr".into(),
        });
        assert_eq!(entries(reply)[0].value(), "none");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn refused_requests_leave_the_file_unchanged() {
        let (root, store) = store("refuse");
        let path = root.join("worker/config.toml");
        std::fs::write(&path, "name = \"a\"\n").unwrap();
        let cases = [
            (
                Request::ConfigSet {
                    key: "nope".into(),
                    value: "1".into(),
                },
                "config.unknown_key",
            ),
            (
                Request::ConfigSet {
                    key: "listen.addr".into(),
                    value: "x".into(),
                },
                "config.invalid_value",
            ),
            (
                Request::ConfigReplace {
                    toml: "name = \"b\"\n".into(),
                    base_hash: file_hash(b""),
                },
                "config.conflict",
            ),
            (
                Request::ConfigReplace {
                    toml: "name =".into(),
                    base_hash: file_hash(b"name = \"a\"\n"),
                },
                "config.parse",
            ),
            (
                Request::AddDeck {
                    code: "c".into(),
                    name: None,
                },
                "control.unsupported",
            ),
        ];
        for (req, want) in cases {
            let name = format!("{req:?}");
            assert_eq!(store.handle(req), Reply::failed(want), "case={name}");
            let text = std::fs::read_to_string(&path).unwrap();
            assert_eq!(text, "name = \"a\"\n", "case={name}");
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn replace_with_the_right_hash_keeps_the_text() {
        let (root, store) = store("replace");
        let text = "# mine\nname = \"b\"\n";
        let got = entries(store.handle(Request::ConfigReplace {
            toml: text.into(),
            base_hash: file_hash(b""),
        }));
        assert_eq!(got.len(), Key::ALL.len());
        assert_eq!(store.current().name(), "b");
        let on_disk = std::fs::read_to_string(root.join("worker/config.toml")).unwrap();
        assert_eq!(on_disk, text, "comments survive");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_bad_reload_keeps_the_last_valid_values() {
        let (root, store) = store("reload");
        let path = root.join("worker/config.toml");
        std::fs::write(&path, "name = \"good\"\n").unwrap();
        store.reload();
        assert_eq!(store.current().name(), "good");
        std::fs::write(&path, "name = \n").unwrap();
        store.reload();
        assert_eq!(store.current().name(), "good", "last valid values");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn an_invalid_file_fails_the_start_with_its_code() {
        let root = std::env::temp_dir().join(format!("latticed-cfg-bad-{}", std::process::id()));
        drop(std::fs::remove_dir_all(&root));
        let worker = root.join("worker");
        let lock = DirLock::acquire(&worker, crate::runtime::uid()).unwrap();
        std::fs::write(worker.join("config.toml"), "colour = \"red\"\n").unwrap();
        let err = ConfigStore::load(worker.join("config.toml"), Layers::new("h".into()), lock)
            .unwrap_err();
        assert!(
            format!("{err:#}").starts_with("config.unknown_key"),
            "err={err:#}"
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}

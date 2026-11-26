//! The deck's key and worker list in `<data dir>/deck` (specs/01-pairing.md, Files).
//!
//! Sync file I/O: call these at start-up or off the hot path; the files are tiny. The deck
//! GUI calls [`DeckFiles::save_workers`] on its engine thread once per pairing: a short
//! block, accepted because this crate picks no executor.

use std::fs::{File, OpenOptions, TryLockError};
use std::io::{Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use lattice_key::{NodeKey, SecretKeyBytes};
use lattice_sync_proto::{PAIR_SECRET_LEN, PairSecret};

use crate::workers::WorkerList;

/// The deck's files: `<data dir>/deck/`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeckFiles {
    dir: PathBuf,
}

impl DeckFiles {
    /// The deck dir at `dir`, `<data dir>/deck` (created with mode 0700). The binaries
    /// resolve the data dir (specs/01-pairing.md, Files): it depends on the OS.
    ///
    /// # Errors
    ///
    /// The dir cannot be created.
    pub fn at(dir: PathBuf) -> std::io::Result<Self> {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&dir)?;
        Ok(Self { dir })
    }

    /// `deck/node.key`, created on first use (mode 0600).
    ///
    /// # Errors
    ///
    /// The file cannot be read or created.
    pub fn node_key(&self) -> std::io::Result<NodeKey> {
        load_or_create_key(&self.dir.join("node.key"))
    }

    /// The worker list; empty when the file does not exist yet.
    ///
    /// # Errors
    ///
    /// The file exists but cannot be read.
    pub fn workers(&self) -> std::io::Result<WorkerList> {
        match std::fs::read_to_string(self.workers_path()) {
            Ok(text) => Ok(WorkerList::parse(&text)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(WorkerList::default()),
            Err(e) => Err(e),
        }
    }

    /// Writes the worker list durably: a unique temporary file, synced, renamed over the
    /// old one, then the dir synced (specs/01-pairing.md, Files). Call it while holding the
    /// [`InviteLock`], so two decks on one data dir do not lose each other's write.
    ///
    /// # Errors
    ///
    /// The file cannot be written.
    pub fn save_workers(&self, list: &WorkerList) -> std::io::Result<()> {
        write_synced(&self.workers_path(), list.to_text().as_bytes())
    }

    /// Takes `deck/invite.lock`: one open invite per data dir (specs/01-pairing.md, rule 2).
    /// The lock holds until the [`InviteLock`] drops, or the process ends.
    ///
    /// # Errors
    ///
    /// [`InviteLockError::Busy`] while another invite holds it.
    pub fn lock_invite(&self) -> Result<InviteLock, InviteLockError> {
        let io = |e: std::io::Error| InviteLockError::Io(e.kind());
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(self.dir.join("invite.lock"))
            .map_err(io)?;
        match file.try_lock() {
            Ok(()) => Ok(InviteLock { _file: file }),
            Err(TryLockError::WouldBlock) => Err(InviteLockError::Busy),
            Err(TryLockError::Error(e)) => Err(io(e)),
        }
    }

    fn workers_path(&self) -> PathBuf {
        self.dir.join("workers")
    }
}

/// The deck's one open invite, held (specs/01-pairing.md, rule 2). Dropping it unlocks.
#[derive(Debug)]
pub struct InviteLock {
    _file: File,
}

/// [`DeckFiles::lock_invite`] failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum InviteLockError {
    /// Another deck process on this data dir has an invite open.
    #[error("another invite is open on this data dir")]
    Busy,
    /// The lock file cannot be opened or locked.
    #[error("invite lock: {0}")]
    Io(std::io::ErrorKind),
}

impl InviteLockError {
    /// Stable code for logs and the UI.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Busy => "pair.invite_busy",
            Self::Io(_) => "pair.invite_lock",
        }
    }
}

/// Makes temporary file names unique within this process.
static TMP_SEQ: AtomicU64 = AtomicU64::new(0);

/// Writes `bytes` to `path` through `.<name>.<pid>.<n>.tmp`: write, sync, rename, sync the dir.
fn write_synced(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let n = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let tmp = path.with_file_name(format!(".{name}.{}.{n}.tmp", std::process::id()));
    let written = (|| -> std::io::Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        std::fs::rename(&tmp, path)
    })();
    if written.is_err() {
        // Best effort: a stale temporary file is harmless; the error below is what counts.
        drop(std::fs::remove_file(&tmp));
    }
    written?;
    match path.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => File::open(dir)?.sync_all(),
        _ => Ok(()),
    }
}

/// A fresh pairing secret from the OS RNG.
///
/// # Errors
///
/// `/dev/urandom` cannot be read.
pub fn random_secret() -> std::io::Result<PairSecret> {
    let mut bytes = [0u8; PAIR_SECRET_LEN];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(PairSecret::new(bytes))
}

/// Reads the node key, or creates it with mode 0600 on first run. Never logs the bytes.
fn load_or_create_key(path: &Path) -> std::io::Result<NodeKey> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut bytes = [0u8; 32];
    match std::fs::File::open(path) {
        Ok(mut f) => f.read_exact(&mut bytes)?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            std::fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(path)?
                .write_all(&bytes)?;
        }
        Err(e) => return Err(e),
    }
    Ok(NodeKey::from_secret(SecretKeyBytes::new(bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pair::PairedWorker;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("lattice-deck-{name}-{}", std::process::id()));
        drop(std::fs::remove_dir_all(&dir));
        dir
    }

    #[test]
    fn the_node_key_is_created_once_then_reloaded() {
        let dir = scratch("key");
        let files = DeckFiles::at(dir.clone()).unwrap();
        let first = files.node_key().unwrap().node_id();
        assert_eq!(files.node_key().unwrap().node_id(), first);
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(dir.join("node.key"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn the_worker_list_survives_a_reload() {
        let dir = scratch("workers");
        let files = DeckFiles::at(dir.clone()).unwrap();
        assert!(files.workers().unwrap().workers().is_empty());
        let mut list = WorkerList::default();
        list.add(PairedWorker {
            id: files.node_key().unwrap().node_id(),
            addr: "10.0.0.2:7441".parse().unwrap(),
            name: "build-01".to_owned(),
        });
        files.save_workers(&list).unwrap();
        assert_eq!(DeckFiles::at(dir.clone()).unwrap().workers().unwrap(), list);
        let names: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert!(
            !names.iter().any(|n| n.ends_with(".tmp")),
            "names={names:?}"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// specs/01-pairing.md, rule 2 and CLI rule 6.
    #[test]
    fn one_invite_is_open_per_data_dir() {
        let dir = scratch("lock");
        let gui = DeckFiles::at(dir.clone()).unwrap();
        let cli = DeckFiles::at(dir.clone()).unwrap();
        let held = gui.lock_invite().unwrap();
        let busy = cli.lock_invite().unwrap_err();
        assert_eq!(
            (busy, busy.code()),
            (InviteLockError::Busy, "pair.invite_busy")
        );
        drop(held);
        assert!(cli.lock_invite().is_ok(), "free once the first invite ends");
        std::fs::remove_dir_all(dir).unwrap();
    }
}

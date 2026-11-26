//! The deck's key and worker list in `<data dir>/deck` (specs/01-pairing.md, Files).
//!
//! Sync file I/O: call these at start-up or off the hot path; the files are tiny.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

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

    /// Writes the worker list (temporary file, then rename).
    ///
    /// # Errors
    ///
    /// The file cannot be written.
    pub fn save_workers(&self, list: &WorkerList) -> std::io::Result<()> {
        let path = self.workers_path();
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, list.to_text())?;
        std::fs::rename(tmp, path)
    }

    fn workers_path(&self) -> PathBuf {
        self.dir.join("workers")
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
        std::fs::remove_dir_all(dir).unwrap();
    }
}

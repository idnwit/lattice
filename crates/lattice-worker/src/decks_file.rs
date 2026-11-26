//! Writing the allowlist file `worker/decks` (specs/01-pairing.md, Files). Sync: callers run
//! it on a blocking thread.
//!
//! Read, add, write a unique temporary file, sync it, rename it over the old file, sync the
//! dir. `worker/decks.lock` is held from read to rename, so two `latticed add-deck` runs do
//! not lose each other's deck.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use lattice_key::NodeId;

use crate::allowlist::Allowlist;

/// Makes temporary file names unique within this process.
static TMP_SEQ: AtomicU64 = AtomicU64::new(0);

/// Adds `deck` named `name` to the allowlist at `path`, durably.
pub(crate) fn add(path: &Path, deck: NodeId, name: &str) -> std::io::Result<()> {
    let lock = open_private(&path.with_extension("lock"), false)?;
    lock.lock()?;
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e),
    };
    let mut list = Allowlist::parse(&text);
    list.add(deck, name);
    write_synced(path, list.to_text().as_bytes())
    // `lock` closes here, which releases it.
}

/// Writes `bytes` to `path` through a unique, synced temporary file and a rename.
fn write_synced(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = tmp_path(path);
    let written = (|| -> std::io::Result<()> {
        let mut file = open_private(&tmp, true)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        std::fs::rename(&tmp, path)
    })();
    if written.is_err() {
        // Best effort: a stale temporary file is harmless, the error below is what counts.
        drop(std::fs::remove_file(&tmp));
    }
    written?;
    match path.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => File::open(dir)?.sync_all(),
        _ => Ok(()),
    }
}

/// `.<name>.<pid>.<n>.tmp` next to `path`.
fn tmp_path(path: &Path) -> PathBuf {
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let n = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
    path.with_file_name(format!(".{name}.{}.{n}.tmp", std::process::id()))
}

/// Opens `path` for writing with mode 0600; `fresh` refuses an existing file.
fn open_private(path: &Path, fresh: bool) -> std::io::Result<File> {
    let mut opts = OpenOptions::new();
    opts.write(true).mode(0o600);
    if fresh {
        opts.create_new(true);
    } else {
        opts.create(true).truncate(false);
    }
    opts.open(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lattice_key::{NodeKey, SecretKeyBytes};

    fn id(seed: u8) -> NodeId {
        NodeKey::from_secret(SecretKeyBytes::new([seed; 32])).node_id()
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("lattice-decks-{name}-{}", std::process::id()));
        drop(std::fs::remove_dir_all(&dir));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn concurrent_adds_keep_every_deck_and_leave_no_temporary_file() {
        let dir = scratch("concurrent");
        let path = dir.join("decks");
        let threads: Vec<_> = (1..=8u8)
            .map(|seed| {
                let path = path.clone();
                std::thread::spawn(move || add(&path, id(seed), "deck").unwrap())
            })
            .collect();
        for t in threads {
            t.join().unwrap();
        }
        let list = Allowlist::parse(&std::fs::read_to_string(&path).unwrap());
        assert_eq!(list.decks().count(), 8);
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

    #[test]
    fn a_write_into_a_missing_dir_fails_and_leaves_nothing() {
        let dir = scratch("missing");
        let path = dir.join("gone").join("decks");
        assert!(add(&path, id(1), "deck").is_err());
        assert!(!path.exists());
        std::fs::remove_dir_all(dir).unwrap();
    }
}

//! Where the deck keeps its files, and small OS lookups (specs/01-pairing.md, Files).

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use lattice_deck_core::{DeckFiles, WorkerList};

/// The saved worker list; empty without a data dir or when it cannot be read.
pub(crate) fn paired_workers(files: Option<&DeckFiles>) -> WorkerList {
    match files.map(DeckFiles::workers) {
        Some(Ok(list)) => list,
        Some(Err(e)) => {
            tracing::warn!(kind = %e.kind(), "worker list unreadable");
            WorkerList::default()
        }
        None => WorkerList::default(),
    }
}

/// Overrides the data dir (specs/01-pairing.md, Files).
const DATA_DIR_ENV: &str = "LATTICE_DATA_DIR";

/// `<data dir>/deck`; `None` when it cannot be made (the deck then runs unpaired).
pub(crate) fn deck_files() -> Option<DeckFiles> {
    let dir = data_dir()?.join("deck");
    match DeckFiles::at(dir) {
        Ok(files) => Some(files),
        Err(e) => {
            tracing::warn!(kind = %e.kind(), "deck data dir unusable");
            None
        }
    }
}

/// `$LATTICE_DATA_DIR`, else the OS data dir. `latticed` and the CLI resolve the same dir.
fn data_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os(DATA_DIR_ENV) {
        return Some(PathBuf::from(dir));
    }
    let home = PathBuf::from(std::env::var_os("HOME")?);
    if cfg!(target_os = "macos") || cfg!(target_os = "ios") {
        return Some(home.join("Library/Application Support/lattice"));
    }
    Some(match std::env::var_os("XDG_DATA_HOME") {
        Some(xdg) => PathBuf::from(xdg).join("lattice"),
        None => home.join(".local/share/lattice"),
    })
}

/// This deck's name, sent in `PairOk`: the host name (`hostname -s`), else `deck`.
pub(crate) fn host_name() -> String {
    std::process::Command::new("hostname")
        .arg("-s")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "deck".to_owned())
}

/// Seconds since the Unix epoch.
pub(crate) fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default()
}

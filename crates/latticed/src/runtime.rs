//! The runtime dir of `latticed` and its `worker.addr` (specs/01-pairing.md, Files).

use std::io::Write;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use anyhow::Context;

/// The file `latticed add-deck` reads the listen port from.
const CARD_FILE: &str = "worker.addr";

/// This process's real user id.
pub(crate) fn uid() -> u32 {
    rustix::process::getuid().as_raw()
}

/// The per-user runtime dir, shared with `latticed-proc` (specs/01-pairing.md, Files).
/// Created with mode 0700 if missing; refused unless private to this user.
pub(crate) fn runtime_dir() -> anyhow::Result<PathBuf> {
    let dir = crate::private_dir(&lattice_worker::ProcClient::runtime_dir(uid()))?;
    check_private(&dir, uid())?;
    Ok(dir)
}

/// A real dir (no symlink), owned by `uid`, that no one else may open. A shared `/tmp`
/// lets another user create `lattice-<uid>` first.
fn check_private(dir: &Path, uid: u32) -> anyhow::Result<()> {
    use std::os::unix::fs::MetadataExt;
    let meta = std::fs::symlink_metadata(dir).with_context(|| format!("stat {}", dir.display()))?;
    anyhow::ensure!(
        meta.is_dir() && meta.uid() == uid && meta.mode() & 0o077 == 0,
        "runtime dir {} is not private: want a dir owned by uid {uid} with mode 0700",
        dir.display()
    );
    Ok(())
}

/// Writes `<node id>@<ip:port>` to `<run>/worker.addr`, mode 0600, through a temporary file
/// and a rename: a reader never sees half a line.
pub(crate) fn publish_card(run: &Path, card: &str) -> anyhow::Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    let path = run.join(CARD_FILE);
    let tmp = path.with_extension("tmp");
    // A leftover from a crash would keep its old mode: start from no file.
    match std::fs::remove_file(&tmp) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
            return Err(e).with_context(|| format!("remove {}", tmp.display()));
        }
        _ => {}
    }
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&tmp)
        .with_context(|| format!("create {}", tmp.display()))?;
    f.write_all(card.as_bytes())
        .and_then(|()| f.sync_all())
        .with_context(|| format!("write {}", tmp.display()))?;
    std::fs::rename(&tmp, &path).with_context(|| format!("write {}", path.display()))
}

/// The port the running `latticed` listens on, from `<run>/worker.addr`.
pub(crate) fn listen_port(run: &Path) -> anyhow::Result<u16> {
    let path = run.join(CARD_FILE);
    let card = std::fs::read_to_string(&path)
        .with_context(|| format!("latticed is not running here (no {})", path.display()))?;
    let addr = card
        .trim()
        .rsplit_once('@')
        .map(|(_, a)| a)
        .unwrap_or_default();
    let addr: SocketAddr = addr.parse().context("bad worker.addr")?;
    Ok(addr.port())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_runtime_dir_others_may_open_is_refused() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("latticed-rundir-{}", std::process::id()));
        drop(std::fs::remove_dir_all(&dir));
        let run = crate::private_dir(&dir).unwrap();
        assert!(check_private(&run, uid()).is_ok());
        assert!(
            check_private(&run, uid().wrapping_add(1)).is_err(),
            "another owner"
        );
        std::fs::set_permissions(&run, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(check_private(&run, uid()).is_err(), "mode 0755");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn worker_addr_is_private_and_gives_back_its_port() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("latticed-card-{}", std::process::id()));
        drop(std::fs::remove_dir_all(&dir));
        let run = crate::private_dir(&dir).unwrap();
        publish_card(&run, &format!("{}@127.0.0.1:7441", "ab".repeat(32))).unwrap();
        let mode = std::fs::metadata(run.join(CARD_FILE))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
        assert_eq!(listen_port(&run).unwrap(), 7441);
        std::fs::remove_dir_all(dir).unwrap();
    }
}

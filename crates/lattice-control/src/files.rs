//! The `worker/` dir lock and atomic file writes (specs/11-control-socket.md, rule 1 step 1;
//! specs/10-settings.md, rule 34). The only I/O of this crate: sync `std` and `flock`.

use std::fs::File;
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

/// File name of the control socket in `worker/` (specs/11-control-socket.md, rule 3).
pub const SOCKET_FILE: &str = "latticed.sock";

/// Overrides the config file path (specs/10-settings.md, rule 32).
pub const CONFIG_ENV: &str = "LATTICE_CONFIG";

/// The config file: `override_path` (the value of [`CONFIG_ENV`]), else
/// `<worker dir>/config.toml`.
pub fn config_path(worker: &Path, override_path: Option<std::ffi::OsString>) -> PathBuf {
    override_path.map_or_else(|| worker.join("config.toml"), PathBuf::from)
}

/// The `worker/` dir could not be locked.
#[derive(Debug, thiserror::Error)]
pub enum LockError {
    /// Another process holds the lock: a `latticed` runs, starts, or a CLI writes.
    #[error("the worker dir is locked by another process")]
    Locked,
    /// A symlink, not a dir, or owned by another user.
    #[error("{} is not a private dir of this user", .0.display())]
    Unsafe(PathBuf),
    /// Creating, opening or locking failed.
    #[error("{}: {source}", .path.display())]
    Io {
        /// The dir.
        path: PathBuf,
        /// The OS error.
        source: std::io::Error,
    },
}

/// The exclusive `flock` on the `worker/` dir. Released when dropped (or the process ends).
#[derive(Debug)]
pub struct DirLock {
    _dir: File,
}

impl DirLock {
    /// Prepares and locks `dir`: creates it and missing parents with mode 0700 (an existing
    /// dir is fine), refuses a symlink or a dir of another user, then takes the exclusive `flock`
    /// without waiting.
    ///
    /// # Errors
    ///
    /// [`LockError::Locked`] when another process holds it; [`LockError::Unsafe`] and
    /// [`LockError::Io`] otherwise.
    pub fn acquire(dir: &Path, uid: u32) -> Result<Self, LockError> {
        let io = |source| LockError::Io {
            path: dir.to_owned(),
            source,
        };
        // Missing parents (the data dir on a first run) are made private too.
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)
            .map_err(io)?;
        let meta = std::fs::symlink_metadata(dir).map_err(io)?;
        if !meta.is_dir() || meta.uid() != uid {
            return Err(LockError::Unsafe(dir.to_owned()));
        }
        // O_NOFOLLOW: a swap to a symlink after the check fails the open.
        use rustix::fs::{Mode, OFlags};
        let flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
        let file =
            File::from(rustix::fs::open(dir, flags, Mode::empty()).map_err(|e| io(e.into()))?);
        match rustix::fs::flock(&file, rustix::fs::FlockOperation::NonBlockingLockExclusive) {
            Ok(()) => Ok(Self { _dir: file }),
            Err(rustix::io::Errno::WOULDBLOCK) => Err(LockError::Locked),
            Err(e) => Err(io(e.into())),
        }
    }

    /// Writes `bytes` to `path` atomically: a temporary file in the same dir, synced, then
    /// renamed over the old file (rule 34). Mode 0600. Taking `&self` proves the caller
    /// holds the lock.
    ///
    /// # Errors
    ///
    /// The I/O error of any step; the old file then stays.
    pub fn write_atomic(&self, path: &Path, bytes: &[u8]) -> std::io::Result<()> {
        let mut tmp = path.as_os_str().to_owned();
        tmp.push(".tmp");
        let tmp = PathBuf::from(tmp);
        // A leftover from a crash would keep its old mode: start from no file.
        match std::fs::remove_file(&tmp) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e),
            _ => {}
        }
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        std::fs::rename(&tmp, path)
    }
}

/// Reads `path`; a missing file reads as empty (it sets nothing).
///
/// # Errors
///
/// Any I/O error other than a missing file.
pub fn read_or_empty(path: &Path) -> std::io::Result<Vec<u8>> {
    match std::fs::read(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("lattice-control-{name}-{}", std::process::id()));
        drop(std::fs::remove_dir_all(&dir));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn uid() -> u32 {
        rustix::process::getuid().as_raw()
    }

    /// specs/10-settings.md, Invariant 3: only the `flock` holder writes.
    #[test]
    fn config_writes_are_serialized() {
        let root = scratch("serial");
        let dir = root.join("worker");
        let lock = DirLock::acquire(&dir, uid()).unwrap();
        assert!(
            matches!(DirLock::acquire(&dir, uid()), Err(LockError::Locked)),
            "a second writer is refused while the first holds the lock"
        );
        let file = dir.join("config.toml");
        lock.write_atomic(&file, b"name = \"a\"\n").unwrap();
        drop(lock);
        let again = DirLock::acquire(&dir, uid()).unwrap();
        again.write_atomic(&file, b"name = \"b\"\n").unwrap();
        assert_eq!(std::fs::read(&file).unwrap(), b"name = \"b\"\n");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn two_concurrent_locks_leave_one_holder() {
        let root = scratch("race");
        let dir = root.join("worker");
        let wins: Vec<bool> = std::thread::scope(|s| {
            let tries: Vec<_> = (0..8)
                .map(|_| s.spawn(|| DirLock::acquire(&dir, uid())))
                .collect();
            // Keep every lock alive until all threads are done.
            let locks: Vec<_> = tries.into_iter().map(|t| t.join().unwrap()).collect();
            locks.iter().map(Result::is_ok).collect()
        });
        assert_eq!(wins.iter().filter(|w| **w).count(), 1, "wins={wins:?}");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_symlink_or_a_dir_of_another_user_is_refused() {
        let root = scratch("unsafe");
        let real = root.join("real");
        std::fs::create_dir(&real).unwrap();
        let link = root.join("worker");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        assert!(matches!(
            DirLock::acquire(&link, uid()),
            Err(LockError::Unsafe(_))
        ));
        let other = uid().wrapping_add(1);
        assert!(matches!(
            DirLock::acquire(&real, other),
            Err(LockError::Unsafe(_))
        ));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_missing_dir_is_created_private_and_files_are_0600() {
        use std::os::unix::fs::PermissionsExt;
        let root = scratch("create");
        let dir = root.join("worker");
        let lock = DirLock::acquire(&dir, uid()).unwrap();
        let mode = std::fs::metadata(&dir).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o700);
        let file = dir.join("config.toml");
        assert_eq!(read_or_empty(&file).unwrap(), b"");
        lock.write_atomic(&file, b"x").unwrap();
        let mode = std::fs::metadata(&file).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
        std::fs::remove_dir_all(root).unwrap();
    }
}

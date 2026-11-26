//! The package cache of `latticed`: verify, compile, and cache app packages (D44).
//!
//! `<data dir>/worker/packages/<hash>.lpkg`, so restore finds a package after a restart.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use lattice_app::PackageHash;
use lattice_package::Package;
use lattice_wasm::{Limits, WasmApp};
use lattice_worker::{AppsRunner, BootEpoch, Installed, PackageLoader};

/// Extension of a cached package file.
const PACKAGE_EXT: &str = "lpkg";

/// Verifies a package, compiles its runner, and caches the bytes (best effort).
///
/// TBD: compile off the core task (it blocks the event loop while cranelift runs).
pub(crate) fn package_loader(engine: lattice_wasm::Engine, cache: PathBuf) -> PackageLoader {
    Arc::new(move |hash, bytes| {
        let installed = load_package(&engine, hash, bytes)?;
        let file = cache.join(format!("{hash}.{PACKAGE_EXT}"));
        if let Err(e) = write_atomic(&file, bytes) {
            tracing::warn!(%hash, error = %e, "package not cached; restore after a restart will fail");
        }
        Ok(installed)
    })
}

/// Package bytes → an installed app with a WASM runner.
fn load_package(
    engine: &lattice_wasm::Engine,
    hash: &PackageHash,
    bytes: &[u8],
) -> Result<Installed, String> {
    let package = Package::verify(hash, bytes).map_err(|e| e.code().to_owned())?;
    let app = WasmApp::load(engine, &package.runner, Limits::default())
        .map_err(|e| e.code().to_owned())?;
    let runner = Arc::new(move |params: &[u8], mode| app.instantiate(params, mode));
    Ok(Installed::new(package.manifest, runner))
}

/// A runner at `epoch` with every package in the cache installed. A file that does not load
/// is skipped and logged.
pub(crate) fn cached_packages(
    cache: &Path,
    loader: &PackageLoader,
    epoch: BootEpoch,
) -> AppsRunner {
    let mut apps = AppsRunner::new(epoch);
    let Ok(entries) = std::fs::read_dir(cache) else {
        return apps;
    };
    for path in entries.filter_map(|e| e.ok().map(|e| e.path())) {
        let hash = path
            .file_stem()
            .and_then(|s| s.to_str())
            .filter(|_| path.extension().is_some_and(|e| e == PACKAGE_EXT))
            .and_then(parse_hash);
        let Some(hash) = hash else { continue };
        match std::fs::read(&path)
            .map_err(|e| e.to_string())
            .and_then(|b| loader(&hash, &b))
        {
            Ok(installed) => apps.install(hash, installed),
            Err(code) => tracing::warn!(path = %path.display(), code, "cached package skipped"),
        }
    }
    apps
}

/// Writes `bytes` to `path` through a temporary file and a rename.
fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(tmp, path)
}

/// 64 lowercase hex digits → a package hash.
fn parse_hash(text: &str) -> Option<PackageHash> {
    if text.len() != 64 {
        return None;
    }
    let mut raw = [0u8; 32];
    for (i, b) in raw.iter_mut().enumerate() {
        *b = u8::from_str_radix(text.get(i * 2..i * 2 + 2)?, 16).ok()?;
    }
    Some(PackageHash::from_bytes(raw))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("latticed-test-{name}-{}", std::process::id()));
        drop(std::fs::remove_dir_all(&dir));
        crate::private_dir(&dir).unwrap()
    }

    #[test]
    fn a_built_in_package_loads_caches_and_reloads_from_the_cache() {
        let engine = lattice_wasm::engine().unwrap();
        let cache = scratch("cache");
        let loader = package_loader(engine, cache.clone());
        let echo = lattice_builtin_apps::by_id("dev.lattice.echo").unwrap();
        let installed = loader(&echo.package(), echo.bytes).unwrap();
        assert_eq!(installed.manifest().id.as_str(), echo.id);
        let file = cache.join(format!("{}.{PACKAGE_EXT}", echo.package()));
        assert!(file.is_file(), "the package is cached by hash");

        let apps = cached_packages(&cache, &loader, BootEpoch::from_unix_millis(0));
        assert!(apps.has_package(&echo.package()));
        std::fs::remove_dir_all(cache).unwrap();
    }

    #[test]
    fn a_package_under_the_wrong_hash_does_not_load() {
        let engine = lattice_wasm::engine().unwrap();
        let echo = lattice_builtin_apps::by_id("dev.lattice.echo").unwrap();
        let other = lattice_builtin_apps::by_id("dev.lattice.terminal").unwrap();
        let err = load_package(&engine, &other.package(), echo.bytes).unwrap_err();
        assert_eq!(err, "package.hash_mismatch");
        assert!(load_package(&engine, &echo.package(), echo.bytes).is_ok());
    }

    #[test]
    fn hashes_parse_from_their_file_names_only() {
        let hash = PackageHash::from_bytes([0xab; 32]);
        assert_eq!(parse_hash(&hash.to_string()), Some(hash));
        assert_eq!(parse_hash("abc"), None);
        assert_eq!(parse_hash(&"zz".repeat(32)), None);
    }
}

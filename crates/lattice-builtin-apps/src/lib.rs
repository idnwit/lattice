//! Built-in app packages, embedded at build time (D44; 06-apps.md, Distribution).
//!
//! `build.rs` builds every `apps/*/` crate's runner and viewer to wasm and packs them with
//! its `app.toml` (`lattice-package`). The deck lists these apps in its `+` menu and serves the
//! bytes to a worker that asks for them (`BlobRequest`). Workers never link this crate: they
//! ship no app.
//!
//! # Usage
//!
//! ```
//! for app in lattice_builtin_apps::PACKAGES {
//!     let _menu_entry = (app.id, app.name);
//! }
//! let echo = lattice_builtin_apps::by_id("dev.lattice.echo");
//! # let _ = echo;
//! ```

use lattice_app::PackageHash;

/// One embedded package.
#[derive(Debug, Clone, Copy)]
pub struct Builtin {
    /// App id (`app.toml` `id`).
    pub id: &'static str,
    /// Display name (`app.toml` `name`).
    pub name: &'static str,
    /// blake3 of [`Builtin::bytes`]: the package id.
    pub hash: [u8; 32],
    /// The package (`lattice_package::Package`, postcard).
    pub bytes: &'static [u8],
}

impl Builtin {
    /// The package id.
    pub fn package(&self) -> PackageHash {
        PackageHash::from_bytes(self.hash)
    }
}

include!(concat!(env!("OUT_DIR"), "/builtin.rs"));

/// The built-in app with this id.
pub fn by_id(id: &str) -> Option<&'static Builtin> {
    PACKAGES.iter().find(|p| p.id == id)
}

/// The built-in package with this hash: what a deck answers a `BlobRequest` with.
pub fn by_hash(hash: &PackageHash) -> Option<&'static Builtin> {
    PACKAGES.iter().find(|p| p.package() == *hash)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_app_under_apps_is_embedded_and_verifies() {
        let ids: Vec<_> = PACKAGES.iter().map(|p| p.id).collect();
        assert_eq!(ids, ["dev.lattice.echo", "dev.lattice.terminal"]);
        for p in PACKAGES {
            let package = lattice_package::Package::verify(&p.package(), p.bytes).unwrap();
            assert_eq!(package.manifest.id.as_str(), p.id);
            assert_eq!(&package.runner[..4], b"\0asm");
            assert_eq!(&package.viewer[..4], b"\0asm");
            assert_eq!(by_hash(&p.package()).map(|b| b.id), Some(p.id));
        }
        let terminal = by_id("dev.lattice.terminal").unwrap();
        assert_eq!(terminal.name, "Terminal");
    }
}

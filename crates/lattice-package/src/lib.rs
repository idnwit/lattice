//! App packages: what a deck ships and a worker runs (D44; 06-apps.md, Distribution, Manifest).
//!
//! A package is one blob: the manifest (from `app.toml`), the runner module (`runner.wasm`),
//! and the viewer module (`viewer.wasm`). Its id is the blake3 hash of its bytes, so
//! a worker that fetched it from a deck checks it before running it ([`Package::verify`]).
//! Built-in apps are packaged at build time (`lattice-builtin-apps`) and embedded in the
//! deck; an app installed later travels the same way.
//!
//! Encoding: postcard of [`Package`] (D19). TBD: chunking for packages over one sync frame
//! (1 MiB, `lattice-sync-proto`); signatures for third-party apps (06-apps.md, Permissions).
//!
//! # Usage
//!
//! ```
//! use lattice_package::{Manifest, Package};
//!
//! # fn run(manifest: Manifest, runner: Vec<u8>, viewer: Vec<u8>) -> Result<(), lattice_package::PackageError> {
//! let bytes = Package::new(manifest, runner, viewer).encode()?;
//! let hash = lattice_package::hash(&bytes);
//! let package = Package::verify(&hash, &bytes)?; // what a worker does with a fetched blob
//! # Ok(()) }
//! ```
//!
//! # Key items
//!
//! - [`Manifest`], [`SpawnTemplate`]: `app.toml`. `Manifest::from_toml` with feature `toml`.
//! - [`Package`]: manifest, runner and viewer modules; [`hash`], [`Package::verify`].
//! - [`PackageError`].

use lattice_app::{AppId, PackageHash};
use serde::{Deserialize, Serialize};

/// An argv element that the worker replaces with the user's login shell (`$SHELL`, else
/// `/bin/sh`). The only expansion: argv is otherwise fixed by the manifest.
pub const SHELL_TOKEN: &str = "$SHELL";

/// Why a package or manifest is refused. Codes are stable.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PackageError {
    /// The bytes do not hash to the id they were fetched for.
    #[error("package hash mismatch")]
    HashMismatch,
    /// The bytes are not a package.
    #[error("package does not decode")]
    Decode,
    /// The package does not encode (a manifest field postcard cannot write).
    #[error("package does not encode")]
    Encode,
    /// `app.toml` is not a valid manifest.
    #[error("bad app.toml: {0}")]
    Manifest(String),
}

impl PackageError {
    /// Stable code.
    pub fn code(&self) -> &'static str {
        match self {
            Self::HashMismatch => "package.hash_mismatch",
            Self::Decode => "package.decode",
            Self::Encode => "package.encode",
            Self::Manifest(_) => "package.manifest",
        }
    }
}

/// `app.toml`: who the app is and which processes it may start.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    /// Reverse-DNS app id; the runner's `App::ID`.
    pub id: AppId,
    /// Name shown in the `+` menu and the app picker.
    pub name: String,
    /// Version text. The hash, not this, identifies a build.
    pub version: String,
    /// `[[spawn]]` templates. An app with none starts no process.
    #[serde(default)]
    pub spawn: Vec<SpawnTemplate>,
}

/// One `[[spawn]]` entry: a fixed argv the runner starts by name (`Ctx::spawn`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpawnTemplate {
    /// Name the runner uses.
    pub name: String,
    /// Program and arguments; [`SHELL_TOKEN`] expands on the worker.
    pub argv: Vec<String>,
}

impl Manifest {
    /// Parses `app.toml`.
    ///
    /// # Errors
    ///
    /// [`PackageError::Manifest`] when the TOML does not parse, a field is missing, the id is
    /// not reverse-DNS, or a template has an empty argv.
    #[cfg(feature = "toml")]
    pub fn from_toml(text: &str) -> Result<Self, PackageError> {
        let manifest: Self =
            toml::from_str(text).map_err(|e| PackageError::Manifest(e.to_string()))?;
        if let Some(t) = manifest.spawn.iter().find(|t| t.argv.is_empty()) {
            return Err(PackageError::Manifest(format!(
                "spawn `{}` has an empty argv",
                t.name
            )));
        }
        Ok(manifest)
    }

    /// The `[[spawn]]` template named `name`.
    pub fn template(&self, name: &str) -> Option<&SpawnTemplate> {
        self.spawn.iter().find(|t| t.name == name)
    }
}

/// One package: the manifest and both modules. A worker runs the runner; a deck the viewer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Package {
    /// The manifest.
    pub manifest: Manifest,
    /// `runner.wasm`: core wasm, ABI v0 (D43).
    pub runner: Vec<u8>,
    /// `viewer.wasm`: core wasm, ABI v0 (D43).
    pub viewer: Vec<u8>,
}

impl Package {
    /// A package of `manifest` and the runner and viewer module bytes.
    pub fn new(manifest: Manifest, runner: Vec<u8>, viewer: Vec<u8>) -> Self {
        Self {
            manifest,
            runner,
            viewer,
        }
    }

    /// The package bytes; [`hash`] of them is its id.
    ///
    /// # Errors
    ///
    /// [`PackageError::Encode`]; not expected for a parsed manifest.
    pub fn encode(&self) -> Result<Vec<u8>, PackageError> {
        lattice_state::encode(self).map_err(|_| PackageError::Encode)
    }

    /// Decodes package bytes fetched as `want`, after checking their hash.
    ///
    /// # Errors
    ///
    /// [`PackageError::HashMismatch`] first, then [`PackageError::Decode`].
    pub fn verify(want: &PackageHash, bytes: &[u8]) -> Result<Self, PackageError> {
        if hash(bytes) != *want {
            return Err(PackageError::HashMismatch);
        }
        lattice_state::decode(bytes).map_err(|_| PackageError::Decode)
    }
}

/// The id of package bytes: their blake3 hash.
pub fn hash(bytes: &[u8]) -> PackageHash {
    PackageHash::from_bytes(*blake3::hash(bytes).as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest() -> Manifest {
        Manifest {
            id: AppId::try_new("dev.lattice.test").unwrap(),
            name: "Test".to_owned(),
            version: "0.1.0".to_owned(),
            spawn: vec![SpawnTemplate {
                name: "shell".to_owned(),
                argv: vec![SHELL_TOKEN.to_owned(), "-l".to_owned()],
            }],
        }
    }

    #[test]
    fn a_package_verifies_against_its_own_hash_only() {
        let bytes = Package::new(manifest(), vec![0, 97, 115, 109], Vec::new())
            .encode()
            .unwrap();
        let id = hash(&bytes);
        let back = Package::verify(&id, &bytes).unwrap();
        assert_eq!(
            back.manifest.template("shell").map(|t| t.argv.len()),
            Some(2)
        );
        let other = hash(b"other");
        assert_eq!(
            Package::verify(&other, &bytes),
            Err(PackageError::HashMismatch)
        );
        let junk = b"junk";
        assert_eq!(
            Package::verify(&hash(junk), junk).unwrap_err().code(),
            "package.decode"
        );
    }

    #[cfg(feature = "toml")]
    #[test]
    fn app_toml_parses_and_an_empty_argv_is_refused() {
        let text = r#"
            id = "dev.lattice.test"
            name = "Test"
            version = "0.1.0"

            [[spawn]]
            name = "shell"
            argv = ["$SHELL", "-l"]
        "#;
        assert_eq!(Manifest::from_toml(text).unwrap(), manifest());
        let empty = text.replace(r#"["$SHELL", "-l"]"#, "[]");
        assert_eq!(
            Manifest::from_toml(&empty).unwrap_err().code(),
            "package.manifest"
        );
        let bad_id = text.replace("dev.lattice.test", "Not An Id");
        assert!(Manifest::from_toml(&bad_id).is_err());
    }

    #[test]
    fn error_codes_are_stable() {
        assert_eq!(PackageError::Encode.code(), "package.encode");
        assert_eq!(PackageError::HashMismatch.code(), "package.hash_mismatch");
    }
}

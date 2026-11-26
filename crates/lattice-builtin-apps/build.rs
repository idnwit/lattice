//! Builds every app under `apps/` to a package and embeds it (D44).
//!
//! For each `apps/<name>/` with an `app.toml`: builds the crate twice for
//! `wasm32-unknown-unknown` (release, `--no-default-features`), once with feature `runner`
//! into `target/apps/runner` and once with `viewer` into `target/apps/viewer`; packs
//! `app.toml`, `runner.wasm`, and `viewer.wasm` (`lattice-package`), writes the package to
//! `OUT_DIR`, and lists it in `builtin.rs`. A nested cargo with its own target dir does not
//! wait on the outer build's lock.

#![allow(clippy::print_stdout, reason = "build scripts talk to cargo on stdout")]

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use lattice_package::{Manifest, Package};

/// Crates every module builds on; a change rebuilds the packages.
const SOURCES: &[&str] = &[
    "apps",
    "crates/lattice-app",
    "crates/lattice-app-guest",
    "crates/lattice-app-macros",
    "crates/lattice-state",
    "crates/lattice-state-macros",
    "Cargo.lock",
];

/// Env the outer cargo sets that must not leak into the wasm build.
const SCRUB: &[&str] = &[
    "CARGO_TARGET_DIR",
    "CARGO_BUILD_TARGET",
    "CARGO_ENCODED_RUSTFLAGS",
    "RUSTFLAGS",
    "RUSTC_WORKSPACE_WRAPPER",
    "CARGO_PRIMARY_PACKAGE",
    "CARGO_MAKEFLAGS",
];

struct App {
    krate: String,
    manifest: Manifest,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR")?).join("../..");
    let root = root.canonicalize()?;
    for src in SOURCES {
        println!("cargo:rerun-if-changed={}", root.join(src).display());
    }
    let apps = discover(&root.join("apps"))?;
    let target = root.join("target/apps");
    if !apps.is_empty() {
        for part in PARTS {
            build_wasm(&root, &target.join(part), &apps, part)?;
        }
    }
    let out = PathBuf::from(std::env::var("OUT_DIR")?);
    let mut list = String::new();
    for app in apps.values() {
        let runner = module(&target, "runner", &app.krate)?;
        let viewer = module(&target, "viewer", &app.krate)?;
        let bytes = Package::new(app.manifest.clone(), runner, viewer).encode()?;
        let hash = lattice_package::hash(&bytes);
        let file = out.join(format!("{}.lpkg", app.manifest.id.as_str()));
        std::fs::write(&file, &bytes)?;
        writeln!(
            list,
            "    Builtin {{ id: {:?}, name: {:?}, hash: {:?}, bytes: include_bytes!({:?}) }},",
            app.manifest.id.as_str(),
            app.manifest.name,
            hash.as_bytes(),
            file.display().to_string(),
        )?;
    }
    let code = format!(
        "/// Every built-in package, in app id order.\npub static PACKAGES: &[Builtin] = &[\n{list}];\n"
    );
    std::fs::write(out.join("builtin.rs"), code)?;
    Ok(())
}

/// The two modules of a package, by feature name.
const PARTS: [&str; 2] = ["runner", "viewer"];

/// The built module of `krate` for `part`.
fn module(target: &Path, part: &str, krate: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let wasm = target
        .join(part)
        .join("wasm32-unknown-unknown/release")
        .join(format!("{}.wasm", krate.replace('-', "_")));
    Ok(std::fs::read(&wasm).map_err(|e| format!("{}: {e}", wasm.display()))?)
}

/// `apps/*/` folders with an `app.toml`, keyed by app id.
fn discover(dir: &Path) -> Result<BTreeMap<String, App>, Box<dyn std::error::Error>> {
    let mut apps = BTreeMap::new();
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        let toml_path = path.join("app.toml");
        if !toml_path.is_file() {
            continue;
        }
        let manifest = Manifest::from_toml(&std::fs::read_to_string(&toml_path)?)
            .map_err(|e| format!("{}: {e}", toml_path.display()))?;
        let cargo: toml::Table = std::fs::read_to_string(path.join("Cargo.toml"))?.parse()?;
        let krate = cargo
            .get("package")
            .and_then(|p| p.get("name"))
            .and_then(toml::Value::as_str)
            .ok_or_else(|| format!("{}: no [package] name", path.display()))?
            .to_owned();
        apps.insert(manifest.id.as_str().to_owned(), App { krate, manifest });
    }
    Ok(apps)
}

/// One nested `cargo build` of every app's `part` (`runner` or `viewer`) for wasm32.
fn build_wasm(
    root: &Path,
    target: &Path,
    apps: &BTreeMap<String, App>,
    part: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());
    let mut cmd = Command::new(cargo);
    cmd.current_dir(root)
        .args(["build", "--release", "--target", "wasm32-unknown-unknown"])
        .arg("--target-dir")
        .arg(target)
        .arg("--no-default-features");
    let mut features = Vec::new();
    for app in apps.values() {
        cmd.args(["-p", &app.krate]);
        features.push(format!("{}/{part}", app.krate));
    }
    cmd.args(["--features", &features.join(",")]);
    for var in SCRUB {
        cmd.env_remove(var);
    }
    let status = cmd.status()?;
    if !status.success() {
        return Err(format!("building the app {part}s for wasm32 failed").into());
    }
    Ok(())
}

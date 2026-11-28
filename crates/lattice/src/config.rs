//! `lattice config list|get|set|reset|edit` (specs/10-settings.md, rules 41–46).
//!
//! Each command sends one request over `<data dir>/worker/latticed.sock` when `latticed`
//! answers (specs/11-control-socket.md). With no daemon, `list` and `get` read the file
//! and the environment; `set`, `reset` and `edit` take the `worker/` lock and write the
//! file themselves (rule 34).

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Context;
use clap::Subcommand;
use lattice_control::{
    CONFIG_ENV, ConfigError, ConfigFile, DirLock, Entry, Key, Layers, LockError, RELAY_ENV, Reply,
    ReplyReader, Request, SOCKET_FILE, config_path, encode_request, file_hash, read_or_empty,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

/// `PAIR_TIMEOUT` + 5 s: the longest wait for a reply (specs/11-control-socket.md, rule 12;
/// proposed).
const REPLY_TIMEOUT: Duration = Duration::from_secs(20);

/// Printed by `list` and `get` without a daemon (rule 41).
const NO_DAEMON: &str = "latticed not running: flags not shown";

/// `lattice config` subcommands.
#[derive(Debug, Subcommand)]
pub(crate) enum ConfigCmd {
    /// Print every key with its resolved value and source.
    List,
    /// Print the resolved value of a key.
    Get {
        /// Dotted key name (`relay.addr`).
        key: String,
    },
    /// Validate and store a value.
    Set {
        /// Dotted key name (`relay.addr`).
        key: String,
        /// The new value (`udp://relay.example.com:7400`, `none`).
        value: String,
    },
    /// Remove a key from the file; the default applies.
    Reset {
        /// Dotted key name.
        key: String,
    },
    /// Edit a copy of the file in `$EDITOR`; replace the file only if it did not change.
    Edit,
}

/// Paths of one worker data dir.
struct Paths {
    worker: PathBuf,
    socket: PathBuf,
    file: PathBuf,
}

/// Runs one `lattice config` command.
pub(crate) async fn run(cmd: ConfigCmd, data_dir: &Path) -> anyhow::Result<()> {
    let worker = data_dir.join("worker");
    let paths = Paths {
        socket: worker.join(SOCKET_FILE),
        file: config_path(&worker, std::env::var_os(CONFIG_ENV)),
        worker,
    };
    match cmd {
        ConfigCmd::List => read_cmd(&paths, Request::ConfigList, None).await,
        ConfigCmd::Get { key } => {
            let parsed: Key = key.parse().map_err(fail)?;
            read_cmd(&paths, Request::ConfigGet { key }, Some(parsed)).await
        }
        ConfigCmd::Set { key, value } => set(&paths, key, value).await,
        ConfigCmd::Reset { key } => {
            let parsed: Key = key.parse().map_err(fail)?;
            let change = move |f: &mut ConfigFile| {
                f.reset(parsed);
                Ok(())
            };
            write_cmd(&paths, Request::ConfigReset { key }, Some(parsed), change).await?;
            Ok(())
        }
        ConfigCmd::Edit => edit(&paths).await,
    }
}

/// `list` and `get`: the daemon's view, else the file and the environment.
async fn read_cmd(paths: &Paths, req: Request, key: Option<Key>) -> anyhow::Result<()> {
    let entries = match ask(&paths.socket, &req).await? {
        Some(reply) => entries(reply)?,
        None => {
            println!("{NO_DAEMON}");
            let resolved = offline_layers()?.resolve(&read_file(&paths.file)?);
            match key {
                Some(key) => resolved.entry(key).cloned().into_iter().collect(),
                None => resolved.entries().to_vec(),
            }
        }
    };
    print(&entries, key.is_some());
    Ok(())
}

/// `set`: warns when the relay changes (rule 43).
async fn set(paths: &Paths, key: String, value: String) -> anyhow::Result<()> {
    let parsed: Key = key.parse().map_err(fail)?;
    let before = current_value(paths, parsed).await?;
    let change = {
        let value = value.clone();
        move |f: &mut ConfigFile| f.set(parsed, &value)
    };
    let after = write_cmd(
        paths,
        Request::ConfigSet { key, value },
        Some(parsed),
        change,
    )
    .await?;
    if parsed == Key::RelayAddr && before.as_deref() != after.first().map(Entry::value) {
        println!("relay changed: paired decks must pair again (lattice add-deck)");
    }
    Ok(())
}

/// The resolved value of `key` as the daemon or the file has it.
async fn current_value(paths: &Paths, key: Key) -> anyhow::Result<Option<String>> {
    let req = Request::ConfigGet {
        key: key.as_str().to_owned(),
    };
    let entries = match ask(&paths.socket, &req).await? {
        Some(reply) => entries(reply)?,
        None => {
            let resolved = offline_layers()?.resolve(&read_file(&paths.file)?);
            resolved.entry(key).cloned().into_iter().collect()
        }
    };
    Ok(entries.first().map(|e| e.value().to_owned()))
}

/// Sends `req` to the daemon, else applies `change` to the file under the lock.
async fn write_cmd(
    paths: &Paths,
    req: Request,
    key: Option<Key>,
    change: impl FnOnce(&mut ConfigFile) -> Result<(), ConfigError> + Send + 'static,
) -> anyhow::Result<Vec<Entry>> {
    if let Some(reply) = ask(&paths.socket, &req).await? {
        let entries = entries(reply)?;
        print(&entries, false);
        return Ok(entries);
    }
    let (worker, file) = (paths.worker.clone(), paths.file.clone());
    let resolved = blocking(move || {
        let lock = lock(&worker)?;
        let mut config = read_file(&file)?;
        change(&mut config).map_err(fail)?;
        lock.write_atomic(&file, config.to_toml().as_bytes())
            .with_context(|| format!("write {}", file.display()))?;
        Ok(offline_layers()?.resolve(&config))
    })
    .await?;
    let entries: Vec<Entry> = match key {
        Some(key) => resolved.entry(key).cloned().into_iter().collect(),
        None => resolved.entries().to_vec(),
    };
    print(&entries, false);
    Ok(entries)
}

/// `edit`: a copy in `$EDITOR`, then a replace guarded by the file's hash (rule 45).
async fn edit(paths: &Paths) -> anyhow::Result<()> {
    let file = paths.file.clone();
    let (toml, base_hash) = blocking(move || {
        let before = read_or_empty(&file).with_context(|| format!("read {}", file.display()))?;
        let toml = edit_copy(&before)?;
        Ok((toml, file_hash(&before)))
    })
    .await?;
    let req = Request::ConfigReplace {
        toml: toml.clone(),
        base_hash,
    };
    if let Some(reply) = ask(&paths.socket, &req).await? {
        print(&entries(reply).map_err(kept)?, false);
        return Ok(());
    }
    let (worker, file) = (paths.worker.clone(), paths.file.clone());
    blocking(move || {
        let lock = lock(&worker)?;
        let now = read_or_empty(&file).with_context(|| format!("read {}", file.display()))?;
        if file_hash(&now) != base_hash {
            return Err(kept(fail(ConfigError::Conflict)));
        }
        let config = ConfigFile::parse(&toml).map_err(|e| kept(fail(e)))?;
        lock.write_atomic(&file, toml.as_bytes())
            .with_context(|| format!("write {}", file.display()))?;
        print(offline_layers()?.resolve(&config).entries(), false);
        Ok(())
    })
    .await
}

/// Writes `before` to a private temporary copy, runs `$EDITOR` on it, returns the text.
fn edit_copy(before: &[u8]) -> anyhow::Result<String> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let copy = std::env::temp_dir().join(format!("lattice-config-{}.toml", std::process::id()));
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&copy)
        .with_context(|| format!("create {}", copy.display()))?;
    f.write_all(before).context("write the copy")?;
    drop(f);
    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_owned());
    // `sh -c` so an EDITOR with arguments (`code -w`) works; the path stays one word.
    let status = std::process::Command::new("sh")
        .arg("-c")
        .arg(format!("{editor} \"$1\""))
        .arg("sh")
        .arg(&copy)
        .status();
    let text = std::fs::read_to_string(&copy);
    // The copy is ours; a failed removal leaves a private file in the temp dir.
    drop(std::fs::remove_file(&copy));
    let status = status.with_context(|| format!("run {editor}"))?;
    anyhow::ensure!(status.success(), "{editor} failed: the old file stays");
    text.context("read the copy")
}

/// The reply's entries, or its code as the error.
fn entries(reply: Reply) -> anyhow::Result<Vec<Entry>> {
    match reply {
        Reply::Config { entries } => Ok(entries),
        Reply::Failed { code } => Err(anyhow::anyhow!("{code}")),
    }
}

/// `key  value  (source)` per line, or the bare value for `get`.
fn print(entries: &[Entry], bare: bool) {
    for e in entries {
        if bare {
            println!("{}", e.value());
        } else {
            println!("{:<17} {} ({})", e.key(), e.value(), e.source());
        }
    }
}

/// One request, one reply. `None` when no daemon answers (`connect()` fails).
///
/// # Errors
///
/// `worker.busy` when the socket closes with no reply, `control.timeout` after
/// [`REPLY_TIMEOUT`] (specs/11-control-socket.md, rules 12–13).
async fn ask(socket: &Path, req: &Request) -> anyhow::Result<Option<Reply>> {
    let mut stream = match UnixStream::connect(socket).await {
        Ok(stream) => stream,
        Err(e)
            // InvalidInput: a path too long for a socket, where `latticed` never serves
            // (specs/11-control-socket.md, rule 5).
            if matches!(
                e.kind(),
                std::io::ErrorKind::NotFound
                    | std::io::ErrorKind::ConnectionRefused
                    | std::io::ErrorKind::InvalidInput
            ) =>
        {
            return Ok(None);
        }
        Err(e) => return Err(e).with_context(|| format!("connect {}", socket.display())),
    };
    let mut out = Vec::new();
    encode_request(req, &mut out).map_err(|e| anyhow::anyhow!(e.code()))?;
    let exchange = async {
        stream.write_all(&out).await?;
        let mut bytes = Vec::new();
        stream.read_to_end(&mut bytes).await?;
        Ok::<_, std::io::Error>(bytes)
    };
    let bytes = match tokio::time::timeout(REPLY_TIMEOUT, exchange).await {
        Err(_) => anyhow::bail!("control.timeout: latticed did not reply"),
        // A reset is a close with no reply.
        Ok(Err(_)) => Vec::new(),
        Ok(Ok(bytes)) => bytes,
    };
    let mut reader = ReplyReader::new();
    reader.push(&bytes);
    match reader.next() {
        Ok(Some(reply)) => Ok(Some(reply)),
        Ok(None) => anyhow::bail!("worker.busy: latticed closed with no reply; retry"),
        Err(e) => anyhow::bail!("{}: bad reply from latticed", e.code()),
    }
}

/// The `worker/` lock; `worker.busy` while a daemon starts or another CLI writes.
fn lock(worker: &Path) -> anyhow::Result<DirLock> {
    let uid = rustix::process::getuid().as_raw();
    DirLock::acquire(worker, uid).map_err(|e| match e {
        LockError::Locked => anyhow::anyhow!("worker.busy: latticed is starting or stuck; retry"),
        other => anyhow::Error::new(other),
    })
}

/// The config file, or an empty one when missing.
fn read_file(path: &Path) -> anyhow::Result<ConfigFile> {
    let bytes = read_or_empty(path).with_context(|| format!("read {}", path.display()))?;
    let text = std::str::from_utf8(&bytes).map_err(|e| fail(ConfigError::Parse(e.to_string())))?;
    ConfigFile::parse(text).map_err(fail)
}

/// The environment layer as this process sees it; no flags (rule 41).
fn offline_layers() -> anyhow::Result<Layers> {
    let layers = Layers::new(host_name());
    match std::env::var(RELAY_ENV) {
        Ok(value) => layers
            .env(Key::RelayAddr, &value)
            .map_err(|e| anyhow::anyhow!("{e} (${RELAY_ENV})")),
        Err(_) => Ok(layers),
    }
}

/// The default worker name: the host name (`hostname -s`), else `worker`, as `latticed`.
fn host_name() -> String {
    std::process::Command::new("hostname")
        .arg("-s")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "worker".to_owned())
}

fn fail(e: ConfigError) -> anyhow::Error {
    anyhow::anyhow!("{e}")
}

/// Adds the note that a refused edit changed nothing (rule 45).
fn kept(e: anyhow::Error) -> anyhow::Error {
    anyhow::anyhow!("{e:#}; the old file stays")
}

/// Runs sync file and process work off the async workers.
async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> anyhow::Result<T> + Send + 'static,
) -> anyhow::Result<T> {
    tokio::task::spawn_blocking(f)
        .await
        .context("blocking task")?
}

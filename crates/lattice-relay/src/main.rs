//! `lattice-relay`: the public UDP relay. Composition root plus its UDP loop.
//!
//! Owner specs: `docs/architecture/01-transport.md` (Relay wire format) and
//! `08-infrastructure.md`. Forwards opaque QUIC packets between registered
//! nodes. Sees NodeIds, addresses, sizes, and timing; never content.
//! All decisions live in the sans-IO `RelayEngine`; this file only moves bytes
//! (`.rules/architecture.md` rule 3: I/O in adapters and binaries).

use anyhow::Context;
use clap::Parser;
use lattice_key::{NodeKey, SecretKeyBytes};
use lattice_relay_proto::{DEFAULT_TTL, EngineOutput, Now, RelayEngine, Token, Transmit};
use std::io::{BufReader, Read};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::net::UdpSocket;

/// How often expired registrations and nonces are dropped (01-transport.md, Relay
/// rules 1 and 7: expiry is checked on every datagram too, so this only bounds memory).
const TICK: Duration = Duration::from_secs(1);
/// Receive buffer: the largest UDP payload, so no datagram is cut (Relay rule 5 drops
/// anything malformed; a cut packet would be forwarded corrupt).
const MAX_DATAGRAM: usize = 65_535;

/// Command-line arguments.
#[derive(Debug, Parser)]
#[command(version, about)]
struct Args {
    /// UDP listen address.
    #[arg(long, default_value = "0.0.0.0:7400")]
    listen: SocketAddr,
    /// Relay key file. Created on first run.
    #[arg(long)]
    key: PathBuf,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let args = Args::parse();
    let key = load_or_create_key(&args.key)?;
    let sock = UdpSocket::bind(args.listen)
        .await
        .with_context(|| format!("bind {}", args.listen))?;
    tracing::info!(addr = %sock.local_addr()?, relay_id = %key.node_id(), "relay listening");
    let mut tokens = TokenSource::open()?;
    // Each relay process draws a fresh cookie secret (01-transport, Relay rule 19).
    let cookie_secret = tokens.secret()?;
    let engine = RelayEngine::new(key.node_id(), DEFAULT_TTL, cookie_secret);
    serve(sock, engine, tokens).await
}

/// Receives datagrams, runs the engine, sends its output. Expires on a timer.
async fn serve(
    sock: UdpSocket,
    mut engine: RelayEngine,
    mut tokens: TokenSource,
) -> anyhow::Result<()> {
    let start = Instant::now();
    let mut buf = vec![0u8; MAX_DATAGRAM];
    let mut tick = tokio::time::interval(TICK);
    loop {
        tokio::select! {
            got = sock.recv_from(&mut buf) => {
                // A public relay keeps serving: an ICMP-driven error on one datagram
                // (e.g. a port unreachable on some OSes) must not stop it.
                let (n, src) = match got {
                    Ok(got) => got,
                    Err(e) => {
                        tracing::debug!(error = %e, "recv failed");
                        continue;
                    }
                };
                let bytes = buf.get(..n).context("recv length above buffer")?;
                let out = engine.handle(now(start), src, bytes, tokens.next()?);
                send_output(&sock, out).await;
            }
            _ = tick.tick() => engine.on_tick(now(start)),
        }
    }
}

/// Both relay clocks: monotonic since start, wall clock in UNIX seconds.
fn now(start: Instant) -> Now {
    // A wall clock before 1970 only makes every REGISTER stale (rule 6): fail closed.
    let unix_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    Now::new(start.elapsed(), unix_secs)
}

/// Sends what the engine decided. UDP is best effort: send errors are logged and dropped.
async fn send_output(sock: &UdpSocket, out: EngineOutput) {
    match out {
        EngineOutput::Drop => {}
        EngineOutput::Reply(t) | EngineOutput::Forward(t) => send(sock, &t).await,
        EngineOutput::Punch(ts) => {
            for t in &ts {
                send(sock, t).await;
            }
        }
    }
}

async fn send(sock: &UdpSocket, t: &Transmit) {
    if let Err(e) = sock.send_to(t.bytes(), t.dst()).await {
        tracing::debug!(dst = %t.dst(), error = %e, "send failed");
    }
}

/// Random tokens from the OS RNG, buffered so a datagram rarely costs a syscall.
struct TokenSource(BufReader<std::fs::File>);

impl TokenSource {
    fn open() -> anyhow::Result<Self> {
        let f = std::fs::File::open("/dev/urandom").context("open /dev/urandom")?;
        Ok(Self(BufReader::with_capacity(4096, f)))
    }

    fn next(&mut self) -> anyhow::Result<Token> {
        let mut b = [0u8; 8];
        self.0.read_exact(&mut b).context("read /dev/urandom")?;
        Ok(Token::from_bytes(b))
    }

    /// 32 random bytes: the cookie secret (01-transport, Relay rule 19).
    fn secret(&mut self) -> anyhow::Result<[u8; 32]> {
        let mut b = [0u8; 32];
        self.0.read_exact(&mut b).context("read /dev/urandom")?;
        Ok(b)
    }
}

/// Reads the relay key, or creates it with mode 0600 on first run. Never logs the bytes.
///
/// Same scheme as `latticed`. TBD: move into `lattice-key` if a third user appears.
fn load_or_create_key(path: &Path) -> anyhow::Result<NodeKey> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut bytes = [0u8; 32];
    if let Ok(mut f) = std::fs::File::open(path) {
        f.read_exact(&mut bytes).context("read relay key")?;
    } else {
        std::fs::File::open("/dev/urandom")
            .and_then(|mut r| r.read_exact(&mut bytes))
            .context("read /dev/urandom")?;
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)
            .with_context(|| format!("create relay key {}", path.display()))?;
        std::io::Write::write_all(&mut f, &bytes).context("write relay key")?;
    }
    Ok(NodeKey::from_secret(SecretKeyBytes::new(bytes)))
}

#[cfg(test)]
mod tests;

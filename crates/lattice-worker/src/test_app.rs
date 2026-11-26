//! A tiny app for host tests: counts output bytes, finishes when its process exits.

use lattice_app::{App, Ctx, ExitStatus, ProcRef};
use lattice_state::Value;

/// Echo app: state = output bytes seen. Remembers the last process it saw.
#[derive(Debug)]
pub(crate) struct Echo {
    proc: Option<ProcRef>,
}

thread_local! {
    /// `resumed` calls on this thread: tests run the sans-IO runner on their own thread.
    pub(crate) static RESUMED: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// The runner adapter hosts register: `hosted::<EchoRunner>`.
pub(crate) type EchoRunner = EchoApiRunner<Echo>;

/// Errors of Echo.
#[derive(Debug, serde::Serialize, serde::Deserialize, thiserror::Error)]
#[serde(rename_all = "snake_case")]
pub(crate) enum EchoError {
    /// Echo refuses every request that is not empty.
    #[error("refused")]
    Refused,
}

/// Echo's one reducer.
#[lattice_app::interface]
pub(crate) trait EchoApi: App<State = Value<u64>, Params = (), Error = EchoError> {
    /// Sets the state to `u64::MAX` and returns it; refuses non-empty bytes.
    fn echo(
        &mut self,
        ctx: &mut Ctx,
        state: &mut Value<u64>,
        req: Vec<u8>,
    ) -> Result<u64, EchoError>;
}

impl EchoApi for Echo {
    fn echo(
        &mut self,
        _ctx: &mut Ctx,
        state: &mut Value<u64>,
        req: Vec<u8>,
    ) -> Result<u64, EchoError> {
        state.set(u64::MAX);
        if req.is_empty() {
            Ok(u64::MAX)
        } else {
            Err(EchoError::Refused)
        }
    }
}

impl App for Echo {
    const ID: &'static str = "test.echo";
    type State = Value<u64>;
    type Params = ();
    type Error = EchoError;

    fn new(_params: &()) -> Self {
        Self { proc: None }
    }

    fn start(&mut self, ctx: &mut Ctx, _state: &mut Value<u64>) -> Result<(), EchoError> {
        ctx.spawn("main");
        Ok(())
    }

    fn on_output(&mut self, _ctx: &mut Ctx, state: &mut Value<u64>, proc: ProcRef, bytes: &[u8]) {
        self.proc = Some(proc);
        let len = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        state.set(state.get().saturating_add(len));
    }

    fn on_exit(
        &mut self,
        ctx: &mut Ctx,
        _state: &mut Value<u64>,
        _proc: ProcRef,
        _status: ExitStatus,
    ) {
        ctx.finish();
    }

    fn on_gap(&mut self, _ctx: &mut Ctx, _state: &mut Value<u64>) {}

    /// Writes `resumed` to the last process seen, so tests see when it runs.
    fn resumed(&mut self, ctx: &mut Ctx, _state: &mut Value<u64>) {
        RESUMED.with(|n| n.set(n.get().saturating_add(1)));
        if let Some(proc) = self.proc {
            ctx.write(proc, b"resumed".to_vec());
        }
    }
}

/// Echo's package for host tests: its manifest (one template, `main` = `/bin/cat`) and an
/// empty runner module; the runner itself is native ([`echo_installed`]).
pub(crate) fn echo_package_bytes() -> Vec<u8> {
    let manifest = lattice_package::Manifest {
        id: echo_id(),
        name: "Echo".to_owned(),
        version: "0".to_owned(),
        spawn: vec![lattice_package::SpawnTemplate {
            name: "main".to_owned(),
            argv: vec!["/bin/cat".to_owned()],
        }],
    };
    lattice_package::Package::new(manifest, Vec::new(), Vec::new())
        .encode()
        .unwrap()
}

/// The app id of Echo.
pub(crate) fn echo_id() -> lattice_app::AppId {
    lattice_app::AppId::try_new("test.echo").unwrap()
}

/// The hash of [`echo_package_bytes`].
pub(crate) fn echo_hash() -> lattice_app::PackageHash {
    lattice_package::hash(&echo_package_bytes())
}

/// Echo installed with its native runner.
pub(crate) fn echo_installed() -> crate::Installed {
    let package = lattice_package::Package::verify(&echo_hash(), &echo_package_bytes()).unwrap();
    crate::Installed::new(
        package.manifest,
        std::sync::Arc::new(crate::runner::hosted::<EchoRunner>),
    )
}

/// A loader that installs Echo from its package bytes.
pub(crate) fn echo_loader() -> crate::PackageLoader {
    std::sync::Arc::new(|hash, bytes| {
        lattice_package::Package::verify(hash, bytes).map_err(|e| e.code().to_owned())?;
        Ok(echo_installed())
    })
}

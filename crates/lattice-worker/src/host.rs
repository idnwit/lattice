//! The worker host's accept loop.

use crate::error::WorkerError;
use crate::policy::PeerPolicy;
use crate::proc_client::ProcClient;
use lattice_transport::Transport;

/// Serves deck connections for one worker.
#[derive(Debug)]
#[allow(dead_code, reason = "L1 skeleton: read once run is implemented")]
pub struct WorkerHost<T> {
    transport: T,
    proc: ProcClient,
    policy: PeerPolicy,
}

impl<T: Transport + 'static> WorkerHost<T> {
    /// A host over `transport`, talking to `latticed-proc` through `proc`.
    pub fn new(transport: T, proc: ProcClient, policy: PeerPolicy) -> Self {
        Self {
            transport,
            proc,
            policy,
        }
    }

    /// Runs until the transport closes.
    ///
    /// # Errors
    ///
    /// [`WorkerError`] when the proc daemon or transport fails for good.
    #[allow(
        clippy::todo,
        reason = "L1 skeleton: body lands with its contract test"
    )]
    pub async fn run(self) -> Result<(), WorkerError> {
        // restart recovery: proc.send(List) to learn existing processes (04, latticed)
        // loop: transport.accept()
        //   policy.permits(peer) else drop the conn and log the NodeId
        //   spawn a task per conn: accept_bi control stream; PeerSession drives it
        //   proc replies fan out to sessions over bounded channels (engineering.md, Async)
        todo!()
    }
}

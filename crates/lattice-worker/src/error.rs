//! Worker host errors.

/// Error from the worker host. Never holds secrets or peer bytes.
#[derive(Debug, thiserror::Error)]
pub enum WorkerError {
    /// `latticed-proc` socket failed or closed.
    #[error("proc daemon: {0}")]
    Proc(String),
    /// IPC frame could not be parsed.
    #[error("proc ipc: {}", .0.code())]
    Ipc(lattice_proc_ipc::FrameError),
    /// Transport failure.
    #[error("transport: {}", .0.code())]
    Transport(lattice_transport::TransportError),
}

impl WorkerError {
    /// Stable code for logs.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Proc(_) => "worker.proc",
            Self::Ipc(e) => e.code(),
            Self::Transport(e) => e.code(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_codes_are_stable_and_namespaced() {
        assert_eq!(WorkerError::Proc(String::new()).code(), "worker.proc");
        let closed = WorkerError::Transport(lattice_transport::TransportError::Closed);
        assert_eq!(closed.code(), "transport.closed");
    }
}

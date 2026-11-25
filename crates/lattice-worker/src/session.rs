//! One deck's sync session, sans-IO (02-sync.md, Wire messages).

use crate::streams::ProcStream;
use lattice_key::NodeId;
use lattice_proc_ipc::{Reply, Request};
use lattice_sync_proto::Message;

/// What the host must do after the session handled an input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionEffect {
    /// Send a message to the deck.
    Send(Message),
    /// Send a request to `latticed-proc`.
    Proc(Request),
}

/// State of one deck connection. Driven by [`crate::WorkerHost`].
///
/// TBD: mapping between raw `Log` seq numbers and proc byte offsets.
#[derive(Debug)]
#[allow(
    dead_code,
    reason = "L1 skeleton: read once the session is implemented"
)]
pub struct PeerSession {
    worker_id: NodeId,
    greeted: bool,
    subscribed: Vec<ProcStream>,
}

impl PeerSession {
    /// A fresh session on worker `worker_id`.
    pub fn new(worker_id: NodeId) -> Self {
        Self {
            worker_id,
            greeted: false,
            subscribed: Vec::new(),
        }
    }

    /// Handles a message from the deck.
    #[allow(
        clippy::todo,
        unused_variables,
        reason = "L1 skeleton: body lands with its contract test"
    )]
    pub fn on_message(&mut self, msg: Message) -> Vec<SessionEffect> {
        // before Hello: anything else → Send(Error{"sync.hello_first"})
        // Hello: reply Hello{worker_id, versions}; mark greeted
        // List: Proc(List) (Streams reply built in on_proc_reply)
        // Subscribe proc/<id>/raw: remember; Proc(Subscribe{id, from_off from have})
        // Unsubscribe: forget; Proc(Unsubscribe)
        // Action input/resize/signal on proc/<id>/raw: Proc(Input|Resize|Signal) + Send(ActionResult ok)
        // Action with an unknown name: Send(ActionResult{error: "sync.unknown_action"})
        // other: Send(Error{"unsupported"})
        todo!()
    }

    /// Handles a reply or output from `latticed-proc`.
    #[allow(
        clippy::todo,
        unused_variables,
        reason = "L1 skeleton: body lands with its contract test"
    )]
    pub fn on_proc_reply(&mut self, reply: Reply) -> Vec<SessionEffect> {
        // Procs: Send(Streams[proc/<id>/raw as Log for each])
        // Output for a subscribed proc: Send(Change{raw bytes})
        // Gap: Send(Gap{stream, new_start}); Exited: Send(Change) with the exit record (TBD shape)
        todo!()
    }

    /// True while the deck reads `stream`.
    pub fn is_subscribed(&self, stream: &ProcStream) -> bool {
        self.subscribed.contains(stream)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lattice_key::{NodeKey, SecretKeyBytes};
    use lattice_proc_ipc::{Offset, ProcId};
    use lattice_sync_proto::{ActionId, Have, SubscribeMode};

    fn id(seed: u8) -> NodeId {
        NodeKey::from_secret(SecretKeyBytes::new([seed; 32])).node_id()
    }

    fn greeted() -> PeerSession {
        let mut s = PeerSession::new(id(1));
        s.on_message(Message::Hello {
            node_id: id(2),
            device_list_head: None,
            versions: vec![1],
        });
        s
    }

    #[test]
    #[ignore = "contract: lands with PeerSession"]
    fn messages_before_hello_get_an_error() {
        let mut s = PeerSession::new(id(1));
        let out = s.on_message(Message::List { prefix: None });
        assert!(
            matches!(out.as_slice(), [SessionEffect::Send(Message::Error { .. })]),
            "got={out:?}"
        );
    }

    #[test]
    #[ignore = "contract: lands with PeerSession"]
    fn subscribe_raw_asks_proc_for_output() {
        let mut s = greeted();
        let stream = "proc/7/raw".parse().unwrap();
        let out = s.on_message(Message::Subscribe {
            stream,
            have: Have::empty(),
            mode: SubscribeMode::Live,
        });
        let want = SessionEffect::Proc(Request::Subscribe {
            id: ProcId::from_raw(7),
            from_off: Offset::from_raw(0),
        });
        assert_eq!(out, vec![want]);
    }

    #[test]
    #[ignore = "contract: lands with PeerSession"]
    fn output_of_a_subscribed_proc_becomes_a_change() {
        let mut s = greeted();
        let stream = "proc/7/raw".parse().unwrap();
        s.on_message(Message::Subscribe {
            stream,
            have: Have::empty(),
            mode: SubscribeMode::Live,
        });
        let proc = ProcStream::new(ProcId::from_raw(7), crate::ProcChannel::Raw);
        assert!(s.is_subscribed(&proc));
        let out = s.on_proc_reply(Reply::Output {
            id: ProcId::from_raw(7),
            off: Offset::from_raw(0),
            bytes: b"hi".to_vec(),
        });
        assert!(
            matches!(out.as_slice(), [SessionEffect::Send(Message::Change(_))]),
            "got={out:?}"
        );
    }

    #[test]
    #[ignore = "contract: lands with PeerSession"]
    fn input_action_writes_to_the_pty() {
        let mut s = greeted();
        let target = "proc/7/raw".parse().unwrap();
        let out = s.on_message(Message::Action {
            target,
            id: ActionId::new(1),
            name: "input".into(),
            payload: b"ls\r".to_vec(),
        });
        assert!(out.contains(&SessionEffect::Proc(Request::Input {
            id: ProcId::from_raw(7),
            bytes: b"ls\r".to_vec()
        })));
    }
}

//! One deck's sync session, sans-IO (02-sync.md, Wire messages; 06-apps.md, Sync to viewers).

use crate::streams::WorkerStream;
use crate::wire;
use lattice_app::{AppsAction, InstanceId, PackageHash};
use lattice_key::NodeId;
use lattice_sync_proto::{ActionId, Have, Message, PROTOCOL_VERSION, StreamId};

/// `Error` code: a message other than `Hello` came first.
pub(crate) const HELLO_FIRST: &str = "sync.hello_first";
/// `Error` code: no common protocol version.
pub(crate) const BAD_VERSION: &str = "sync.version";
/// `Error` code: a message type the worker does not serve (02-sync.md, Wire messages rule 5).
pub(crate) const UNSUPPORTED: &str = "sync.unsupported";
/// `Error` / `ActionResult` code: the stream is not one the worker owns.
pub(crate) const NO_STREAM: &str = "sync.no_stream";
/// `ActionResult` code: the action payload does not decode.
pub(crate) const BAD_ACTION: &str = "sync.bad_action";

/// What the host must do after the session handled an input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionEffect {
    /// Send a message to the deck.
    Send(Message),
    /// Run an action on `worker/apps` through the apps runner; answer with `ActionResult`.
    Apps(ActionId, AppsAction),
    /// Run an app action on one instance; answer with `ActionResult`.
    Instance(ActionId, InstanceId, Vec<u8>),
    /// Send the deck what it lacks of a state stream: deltas after `have`, or the full state.
    Resume(WorkerStream, Option<u64>),
    /// Answer `List` with `Streams`: the worker's streams under this prefix (all when `None`).
    List(Option<StreamId>),
    /// A package the deck sent in answer to `BlobRequest` (06-apps.md, Distribution rule 6).
    Blob(PackageHash, Vec<u8>),
}

/// State of one deck connection. Driven by [`crate::WorkerHost`].
#[derive(Debug)]
pub struct PeerSession {
    worker_id: NodeId,
    greeted: bool,
    subscribed: Vec<WorkerStream>,
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
    pub fn on_message(&mut self, msg: Message) -> Vec<SessionEffect> {
        if !self.greeted && !matches!(msg, Message::Hello { .. }) {
            return vec![error(HELLO_FIRST)];
        }
        let effect = match msg {
            Message::Hello { versions, .. } => self.on_hello(&versions),
            Message::List { prefix } => SessionEffect::List(prefix),
            Message::Subscribe { stream, have, .. } => self.on_subscribe(&stream, &have),
            Message::Unsubscribe { stream } => {
                if let Some(s) = WorkerStream::parse(&stream) {
                    self.subscribed.retain(|x| *x != s);
                }
                return Vec::new();
            }
            Message::Action {
                target,
                id,
                payload,
                ..
            } => on_action(&target, id, payload),
            // Flow control: State streams send every delta; nothing to coalesce yet.
            Message::Ack { .. } => return Vec::new(),
            Message::Blob { hash, bytes } => {
                SessionEffect::Blob(PackageHash::from_bytes(hash), bytes)
            }
            _ => error(UNSUPPORTED),
        };
        vec![effect]
    }

    /// True while the deck reads `stream`. The host fans deltas out to these sessions only.
    pub fn is_subscribed(&self, stream: &WorkerStream) -> bool {
        self.subscribed.contains(stream)
    }

    fn on_hello(&mut self, versions: &[u16]) -> SessionEffect {
        if !versions.contains(&PROTOCOL_VERSION) {
            return error(BAD_VERSION);
        }
        self.greeted = true;
        SessionEffect::Send(Message::Hello {
            node_id: self.worker_id,
            device_list_head: None,
            versions: vec![PROTOCOL_VERSION],
        })
    }

    fn on_subscribe(&mut self, stream: &StreamId, have: &Have) -> SessionEffect {
        let Some(s) = WorkerStream::parse(stream) else {
            return error(NO_STREAM);
        };
        if !self.subscribed.contains(&s) {
            self.subscribed.push(s);
        }
        let version = have.get(&self.worker_id).map(wire::seq_raw);
        SessionEffect::Resume(s, version)
    }
}

/// Routes an action to the apps runner by its target stream.
fn on_action(target: &StreamId, id: ActionId, payload: Vec<u8>) -> SessionEffect {
    match WorkerStream::parse(target) {
        Some(WorkerStream::Apps) => match lattice_state::decode::<AppsAction>(&payload) {
            Ok(action) => SessionEffect::Apps(id, action),
            Err(_) => action_error(id, BAD_ACTION),
        },
        Some(WorkerStream::State(iid)) => SessionEffect::Instance(id, iid, payload),
        None => action_error(id, NO_STREAM),
    }
}

fn error(code: &str) -> SessionEffect {
    SessionEffect::Send(Message::Error {
        code: code.to_owned(),
    })
}

fn action_error(id: ActionId, code: &str) -> SessionEffect {
    SessionEffect::Send(Message::ActionResult {
        id,
        error: Some(code.to_owned()),
        result: Vec::new(),
        detail: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use lattice_key::{NodeKey, SecretKeyBytes};
    use lattice_sync_proto::{Have, SubscribeMode};

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
    fn messages_before_hello_get_an_error() {
        let mut s = PeerSession::new(id(1));
        let out = s.on_message(Message::List { prefix: None });
        assert!(
            matches!(out.as_slice(), [SessionEffect::Send(Message::Error { .. })]),
            "got={out:?}"
        );
    }

    #[test]
    fn subscribe_without_version_resumes_from_full_state() {
        let mut s = greeted();
        let out = s.on_message(Message::Subscribe {
            stream: "inst/7/state".parse().unwrap(),
            have: Have::empty(),
            mode: SubscribeMode::Live,
        });
        let stream = WorkerStream::State(InstanceId::from_raw(7));
        assert_eq!(out, vec![SessionEffect::Resume(stream, None)]);
        assert!(s.is_subscribed(&stream));
    }

    #[test]
    fn instance_action_goes_to_its_runner() {
        let mut s = greeted();
        let out = s.on_message(Message::Action {
            target: "inst/7/state".parse().unwrap(),
            id: ActionId::new(1),
            name: "input".into(),
            payload: b"ls\r".to_vec(),
        });
        assert_eq!(
            out,
            vec![SessionEffect::Instance(
                ActionId::new(1),
                InstanceId::from_raw(7),
                b"ls\r".to_vec()
            )]
        );
    }
}

#[cfg(test)]
mod more_tests {
    use super::*;
    use lattice_key::{NodeKey, SecretKeyBytes};
    use lattice_sync_proto::{Seq, SubscribeMode};

    fn id(seed: u8) -> NodeId {
        NodeKey::from_secret(SecretKeyBytes::new([seed; 32])).node_id()
    }

    fn hello(versions: Vec<u16>) -> Message {
        Message::Hello {
            node_id: id(2),
            device_list_head: None,
            versions,
        }
    }

    #[test]
    fn hello_is_answered_with_the_worker_id() {
        let mut s = PeerSession::new(id(1));
        let out = s.on_message(hello(vec![PROTOCOL_VERSION]));
        assert!(
            matches!(out.as_slice(), [SessionEffect::Send(Message::Hello { node_id, .. })] if *node_id == id(1))
        );
        let mut s = PeerSession::new(id(1));
        let out = s.on_message(hello(vec![99]));
        assert_eq!(out, vec![error(BAD_VERSION)]);
    }

    #[test]
    fn subscribe_with_a_version_resumes_from_it_and_unsubscribe_forgets() {
        let mut s = PeerSession::new(id(1));
        s.on_message(hello(vec![PROTOCOL_VERSION]));
        let mut have = Have::empty();
        have.advance(id(1), Seq::new(4));
        let out = s.on_message(Message::Subscribe {
            stream: "worker/apps".parse().unwrap(),
            have,
            mode: SubscribeMode::Live,
        });
        assert_eq!(
            out,
            vec![SessionEffect::Resume(WorkerStream::Apps, Some(4))]
        );
        s.on_message(Message::Unsubscribe {
            stream: "worker/apps".parse().unwrap(),
        });
        assert!(!s.is_subscribed(&WorkerStream::Apps));
    }

    #[test]
    fn apps_actions_decode_and_bad_ones_get_an_action_result() {
        let mut s = PeerSession::new(id(1));
        s.on_message(hello(vec![PROTOCOL_VERSION]));
        let close = AppsAction::Close {
            iid: InstanceId::from_raw(3),
        };
        let action = |payload| Message::Action {
            target: "worker/apps".parse().unwrap(),
            id: ActionId::new(5),
            name: "apps".into(),
            payload,
        };
        let out = s.on_message(action(lattice_state::encode(&close).unwrap()));
        assert_eq!(out, vec![SessionEffect::Apps(ActionId::new(5), close)]);
        let out = s.on_message(action(vec![0xff]));
        assert_eq!(out, vec![action_error(ActionId::new(5), BAD_ACTION)]);
        let out = s.on_message(Message::List { prefix: None });
        assert_eq!(out, vec![SessionEffect::List(None)]);
        let out = s.on_message(Message::BlobRequest { hash: [0; 32] });
        assert_eq!(out, vec![error(UNSUPPORTED)]);
    }
}

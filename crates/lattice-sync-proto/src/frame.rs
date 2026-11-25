//! Length-prefixed postcard framing.

use crate::message::Message;

// TBD: maximum frame length (02-sync.md)? Bounds the decoder buffer.

/// Error that ends a channel: framing is lost.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FrameError {
    /// Declared length above the limit.
    #[error("frame too long")]
    TooLong,
    /// Known message type with a malformed body.
    #[error("malformed message")]
    Malformed,
}

impl FrameError {
    /// Stable code for logs and `Error` messages.
    pub fn code(&self) -> &'static str {
        match self {
            Self::TooLong => "sync.too_long",
            Self::Malformed => "sync.malformed",
        }
    }
}

/// One decoded frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decoded {
    /// A known message.
    Message(Message),
    /// A newer peer's message type. Reply `Error{unsupported}`, keep going (02, rule 5).
    Unsupported,
}

/// Appends `u32 len | postcard(msg)` to `out`.
#[allow(
    clippy::todo,
    unused_variables,
    clippy::ptr_arg,
    reason = "L1 skeleton: body lands with its contract test"
)]
pub fn encode(msg: &Message, out: &mut Vec<u8>) {
    // reserve 4 bytes; postcard::to_extend the message; patch the length (TBD byte order)
    todo!()
}

/// Splits a byte stream into messages. Sans-IO.
#[derive(Debug, Default)]
#[allow(
    dead_code,
    reason = "L1 skeleton: read once the decoder is implemented"
)]
pub struct FrameDecoder {
    buf: Vec<u8>,
}

impl FrameDecoder {
    /// An empty decoder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends bytes read from a stream.
    pub fn push(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(bytes);
    }

    /// Next frame, `Ok(None)` when more bytes are needed.
    ///
    /// # Errors
    ///
    /// [`FrameError`] when the channel must close.
    #[allow(
        clippy::todo,
        unused_variables,
        reason = "L1 skeleton: body lands with its contract test"
    )]
    #[allow(clippy::should_implement_trait, reason = "fallible; not an Iterator")]
    pub fn next(&mut self) -> Result<Option<Decoded>, FrameError> {
        // need 4 bytes for len; len above limit: TooLong
        // need len body bytes, else Ok(None)
        // peek the postcard varint variant tag: above the last known variant → Unsupported
        // else postcard::from_bytes; failure → Malformed
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ActionId, Have, SubscribeMode};

    fn roundtrip(msg: Message) {
        let mut wire = Vec::new();
        encode(&msg, &mut wire);
        let mut dec = FrameDecoder::new();
        // Byte by byte: proves partial frames wait for more input.
        for b in &wire {
            dec.push(std::slice::from_ref(b));
        }
        assert_eq!(
            dec.next(),
            Ok(Some(Decoded::Message(msg.clone()))),
            "case={msg:?}"
        );
    }

    #[test]
    fn error_codes_are_stable_and_namespaced() {
        assert_eq!(FrameError::TooLong.code(), "sync.too_long");
        assert_eq!(FrameError::Malformed.code(), "sync.malformed");
    }

    #[test]
    #[ignore = "contract: lands with encode / FrameDecoder::next"]
    fn messages_roundtrip_across_split_reads() {
        let stream: crate::StreamId = "proc/1/raw".parse().unwrap();
        roundtrip(Message::Subscribe {
            stream: stream.clone(),
            have: Have::empty(),
            mode: SubscribeMode::Live,
        });
        roundtrip(Message::Action {
            target: stream,
            id: ActionId::new(1),
            name: "input".into(),
            payload: b"ls\r".to_vec(),
        });
        roundtrip(Message::Error {
            code: "unsupported".into(),
        });
        let node_id =
            lattice_key::NodeKey::from_secret(lattice_key::SecretKeyBytes::new([1; 32])).node_id();
        roundtrip(Message::Hello {
            node_id,
            device_list_head: None,
            versions: vec![crate::PROTOCOL_VERSION],
        });
    }

    #[test]
    #[ignore = "contract: lands with FrameDecoder::next"]
    fn unknown_message_type_is_unsupported_not_fatal() {
        let mut dec = FrameDecoder::new();
        // len 1, body = variant tag 0x7F (no such message).
        dec.push(&[0, 0, 0, 1, 0x7F]);
        assert_eq!(dec.next(), Ok(Some(Decoded::Unsupported)));
    }
}

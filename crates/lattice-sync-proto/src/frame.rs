//! Length-prefixed postcard framing.

use crate::message::{MESSAGE_TAGS, Message};

/// Max frame body length, in bytes: 1 MiB (02-sync.md, Wire messages; proposed).
/// Bounds the decoder buffer. Larger payloads (blobs) need chunking (TBD).
pub const MAX_FRAME_LEN: usize = 1 << 20;

/// Bytes of the big-endian `u32` length prefix.
const LEN_PREFIX: usize = 4;

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

/// Appends `u32 len (big-endian) | postcard(msg)` to `out`.
///
/// # Errors
///
/// [`FrameError::TooLong`] when the body exceeds [`MAX_FRAME_LEN`];
/// [`FrameError::Malformed`] if postcard cannot encode `msg`. `out` is
/// unchanged on error.
pub fn encode(msg: &Message, out: &mut Vec<u8>) -> Result<(), FrameError> {
    let start = out.len();
    out.extend_from_slice(&[0; LEN_PREFIX]);
    let body_start = out.len();
    if postcard::to_extend(msg, VecTail(out)).is_err() {
        out.truncate(start);
        return Err(FrameError::Malformed);
    }
    let body_len = out.len().saturating_sub(body_start);
    let prefix = match u32::try_from(body_len) {
        Ok(len) if body_len <= MAX_FRAME_LEN => len.to_be_bytes(),
        _ => {
            out.truncate(start);
            return Err(FrameError::TooLong);
        }
    };
    if let Some(slot) = out.get_mut(start..body_start) {
        slot.copy_from_slice(&prefix);
    }
    Ok(())
}

/// Lets postcard append to a borrowed `Vec`.
struct VecTail<'a>(&'a mut Vec<u8>);

impl Extend<u8> for VecTail<'_> {
    fn extend<I: IntoIterator<Item = u8>>(&mut self, iter: I) {
        self.0.extend(iter);
    }
}

/// Splits a byte stream into messages. Sans-IO.
///
/// Holds at most one partial frame plus whatever the caller pushed; call
/// [`FrameDecoder::next`] until `Ok(None)` after each push.
#[derive(Debug, Default)]
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
    /// An unknown message type is consumed and returned as
    /// [`Decoded::Unsupported`]; the stream stays usable.
    ///
    /// # Errors
    ///
    /// [`FrameError`] when the channel must close.
    #[allow(clippy::should_implement_trait, reason = "fallible; not an Iterator")]
    pub fn next(&mut self) -> Result<Option<Decoded>, FrameError> {
        let Some((prefix, rest)) = self.buf.split_first_chunk::<LEN_PREFIX>() else {
            return Ok(None);
        };
        let len = usize::try_from(u32::from_be_bytes(*prefix)).map_err(|_| FrameError::TooLong)?;
        if len > MAX_FRAME_LEN {
            return Err(FrameError::TooLong);
        }
        let Some(body) = rest.get(..len) else {
            return Ok(None);
        };
        let decoded = decode_body(body)?;
        // LEN_PREFIX + len <= buf.len(), checked by the `get` above.
        self.buf.drain(..LEN_PREFIX.saturating_add(len));
        Ok(Some(decoded))
    }
}

/// Decodes one frame body. Unknown tags are not errors (02, rule 5).
fn decode_body(body: &[u8]) -> Result<Decoded, FrameError> {
    let tag = read_varint_u32(body).ok_or(FrameError::Malformed)?;
    if tag >= MESSAGE_TAGS {
        return Ok(Decoded::Unsupported);
    }
    match postcard::take_from_bytes::<Message>(body) {
        // Trailing bytes mean the peer and we disagree on the shape: not ours to guess.
        Ok((msg, [])) => Ok(Decoded::Message(msg)),
        _ => Err(FrameError::Malformed),
    }
}

/// Postcard's enum tag: a LEB128 `u32` varint (at most 5 bytes).
fn read_varint_u32(bytes: &[u8]) -> Option<u32> {
    let mut value: u32 = 0;
    for (i, &b) in bytes.iter().take(5).enumerate() {
        let shift = u32::try_from(i).ok()?.checked_mul(7)?;
        let part = u32::from(b & 0x7F).checked_shl(shift)?;
        value |= part;
        if b & 0x80 == 0 {
            return Some(value);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ActionId, Have, SubscribeMode};

    fn roundtrip(msg: Message) {
        let mut wire = Vec::new();
        encode(&msg, &mut wire).unwrap();
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
    fn messages_roundtrip_across_split_reads() {
        let stream: crate::StreamId = "inst/1/state".parse().unwrap();
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
    fn unknown_message_type_is_unsupported_not_fatal() {
        let mut dec = FrameDecoder::new();
        // len 1, body = variant tag 0x7F (no such message).
        dec.push(&[0, 0, 0, 1, 0x7F]);
        assert_eq!(dec.next(), Ok(Some(Decoded::Unsupported)));
        // The channel keeps going: the next frame still decodes.
        let msg = Message::Error {
            code: "unsupported".into(),
        };
        let mut wire = Vec::new();
        encode(&msg, &mut wire).unwrap();
        dec.push(&wire);
        assert_eq!(dec.next(), Ok(Some(Decoded::Message(msg))));
        assert_eq!(dec.next(), Ok(None));
    }

    #[test]
    fn frame_prefix_is_u32_big_endian() {
        let mut wire = Vec::new();
        encode(&Message::List { prefix: None }, &mut wire).unwrap();
        // List = tag 1, prefix None = 0.
        assert_eq!(wire, [0, 0, 0, 2, 1, 0]);
    }

    #[test]
    fn two_frames_in_one_push_decode_in_order() {
        let a = Message::Unsubscribe {
            stream: "worker/apps".parse().unwrap(),
        };
        let b = Message::BlobRequest { hash: [7; 32] };
        let mut wire = Vec::new();
        encode(&a, &mut wire).unwrap();
        encode(&b, &mut wire).unwrap();
        let mut dec = FrameDecoder::new();
        dec.push(&wire);
        assert_eq!(dec.next(), Ok(Some(Decoded::Message(a))));
        assert_eq!(dec.next(), Ok(Some(Decoded::Message(b))));
        assert_eq!(dec.next(), Ok(None));
    }

    #[test]
    fn declared_length_above_max_is_too_long() {
        let mut dec = FrameDecoder::new();
        let len = u32::try_from(MAX_FRAME_LEN + 1).unwrap();
        dec.push(&len.to_be_bytes());
        assert_eq!(dec.next(), Err(FrameError::TooLong));
    }

    #[test]
    fn encode_refuses_frames_above_max() {
        let mut wire = vec![9];
        let big = Message::Blob {
            hash: [0; 32],
            bytes: vec![0; MAX_FRAME_LEN],
        };
        assert_eq!(encode(&big, &mut wire), Err(FrameError::TooLong));
        assert_eq!(wire, [9], "out is unchanged on error");
    }

    #[test]
    fn malformed_bodies_are_errors() {
        let cases: [(&str, &[u8]); 4] = [
            ("empty body", &[0, 0, 0, 0]),
            ("truncated known message", &[0, 0, 0, 1, 3]),
            ("trailing bytes", &[0, 0, 0, 3, 1, 0, 0]),
            ("invalid stream id", &[0, 0, 0, 3, 8, 1, b'/']),
        ];
        for (name, wire) in cases {
            let mut dec = FrameDecoder::new();
            dec.push(wire);
            assert_eq!(dec.next(), Err(FrameError::Malformed), "case={name}");
        }
    }

    #[test]
    fn partial_prefix_waits_for_more() {
        let mut dec = FrameDecoder::new();
        dec.push(&[0, 0]);
        assert_eq!(dec.next(), Ok(None));
        dec.push(&[0, 2, 1]);
        assert_eq!(dec.next(), Ok(None));
        dec.push(&[0]);
        assert_eq!(
            dec.next(),
            Ok(Some(Decoded::Message(Message::List { prefix: None })))
        );
    }
}

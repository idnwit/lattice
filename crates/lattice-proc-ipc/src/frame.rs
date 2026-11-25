//! Frame header, TLV fields, and the incremental decoder.

#[allow(
    dead_code,
    reason = "L1 skeleton: read by the decoder once implemented"
)]
/// Bytes before the TLV payload: `u32 len | u16 type | u16 flags`.
pub(crate) const HEADER_LEN: usize = 8;

// TBD: maximum frame length? Needed to bound the decoder (04-worker.md).
// TBD: byte order of header and TLV integers (big or little endian)?

/// Message type code of a frame.
#[allow(dead_code, reason = "L1 skeleton: read by the codec once implemented")]
///
/// TBD: v1 numeric codes per request and reply (04-worker.md table has names only).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MsgType(u16);

#[allow(dead_code, reason = "L1 skeleton: read by the codec once implemented")]
impl MsgType {
    /// Wraps a raw type code read from the wire. Unknown codes are kept, not rejected.
    pub(crate) fn from_raw(raw: u16) -> Self {
        Self(raw)
    }

    /// The raw type code.
    pub(crate) fn raw(self) -> u16 {
        self.0
    }
}

/// One decoded frame: type, flags, and its raw TLV payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    msg_type: MsgType,
    flags: u16,
    payload: Vec<u8>,
}

#[allow(dead_code, reason = "L1 skeleton: read by the codec once implemented")]
impl Frame {
    /// The frame's type code.
    pub(crate) fn msg_type(&self) -> MsgType {
        self.msg_type
    }

    /// The frame's flags (none defined in v1).
    pub(crate) fn flags(&self) -> u16 {
        self.flags
    }

    /// Iterates the TLV fields of the payload.
    pub(crate) fn tlvs(&self) -> TlvIter<'_> {
        TlvIter {
            rest: &self.payload,
        }
    }
}

/// One TLV field borrowed from a frame payload.
#[allow(dead_code, reason = "L1 skeleton: read by the codec once implemented")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Tlv<'a> {
    tag: u16,
    value: &'a [u8],
}

#[allow(dead_code, reason = "L1 skeleton: read by the codec once implemented")]
impl<'a> Tlv<'a> {
    /// The field tag. Unknown tags are skipped by message decoders.
    pub(crate) fn tag(&self) -> u16 {
        self.tag
    }

    /// The field bytes.
    pub(crate) fn value(&self) -> &'a [u8] {
        self.value
    }
}

/// Iterator over TLV fields. Yields an error once on a truncated field, then stops.
#[derive(Debug)]
#[allow(dead_code, reason = "L1 skeleton: read by next() once implemented")]
pub(crate) struct TlvIter<'a> {
    rest: &'a [u8],
}

impl<'a> Iterator for TlvIter<'a> {
    type Item = Result<Tlv<'a>, FrameError>;

    #[allow(
        clippy::todo,
        reason = "L1 skeleton: body lands with its contract test"
    )]
    fn next(&mut self) -> Option<Self::Item> {
        // empty rest: return None
        // read u16 tag + u32 len with checked slicing; short header: Truncated, clear rest
        // len past the end of rest: Truncated, clear rest
        // split value off rest and return Tlv { tag, value }
        todo!()
    }
}

/// Error from splitting or parsing frames. Never holds peer bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameError {
    /// A declared length exceeds the frame limit.
    TooLong,
    /// A TLV field or header ends early.
    Truncated,
    /// A required tag is missing for a known type.
    MissingTag(u16),
    /// A field value has the wrong size or an invalid value.
    BadValue(u16),
}

impl FrameError {
    /// Stable code for logs and `Error` replies.
    pub fn code(&self) -> &'static str {
        match self {
            Self::TooLong => "ipc.too_long",
            Self::Truncated => "ipc.truncated",
            Self::MissingTag(_) => "ipc.missing_tag",
            Self::BadValue(_) => "ipc.bad_value",
        }
    }
}

/// Splits a byte stream into frames. Sans-IO: the caller feeds socket reads.
#[derive(Debug, Default)]
pub struct FrameDecoder {
    buf: Vec<u8>,
}

impl FrameDecoder {
    /// An empty decoder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends bytes read from the socket.
    pub fn push(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(bytes);
    }

    /// Returns the next complete frame, `Ok(None)` when more bytes are needed.
    ///
    /// # Errors
    ///
    /// [`FrameError::TooLong`] when the header declares a length above the limit.
    /// The connection must then be closed: framing is lost.
    #[allow(
        clippy::todo,
        reason = "L1 skeleton: body lands with its contract test"
    )]
    pub fn next_frame(&mut self) -> Result<Option<Frame>, FrameError> {
        // fewer than HEADER_LEN bytes buffered: Ok(None)
        // parse len, type, flags; len above the max frame length: Err(TooLong)
        // payload not complete yet: Ok(None)
        // drain header + payload from buf and return the Frame
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "contract: lands with FrameDecoder::next_frame"]
    fn decoder_yields_none_until_a_frame_is_complete() {
        let mut dec = FrameDecoder::new();
        dec.push(&[0, 0, 0]);
        assert_eq!(dec.next_frame(), Ok(None));
    }

    #[test]
    #[ignore = "contract: lands with TlvIter::next"]
    fn tlv_iter_reports_truncated_field_once() {
        let frame = Frame {
            msg_type: MsgType::from_raw(0),
            flags: 0,
            payload: vec![0, 1, 0],
        };
        let got: Vec<_> = frame.tlvs().collect();
        assert_eq!(got, vec![Err(FrameError::Truncated)]);
    }
}

//! Frame header, TLV fields, and the incremental decoder.
//!
//! v1 wire rules (04-worker.md, Local IPC): big-endian integers everywhere;
//! header `u32 len | u16 type | u16 flags`; `len` counts the payload only and is
//! at most [`MAX_FRAME_LEN`].

/// Bytes before the TLV payload: `u32 len | u16 type | u16 flags`.
pub(crate) const HEADER_LEN: usize = 8;

/// Bytes before a TLV value: `u16 tag | u32 len`.
const TLV_HEADER_LEN: usize = 6;

/// Largest payload a frame may declare: 1 MiB. Above it the decoder fails with `TooLong`.
pub const MAX_FRAME_LEN: usize = 1 << 20;

/// Message type code of a frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MsgType(u16);

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

impl Frame {
    /// The frame's type code.
    pub(crate) fn msg_type(&self) -> MsgType {
        self.msg_type
    }

    /// The frame's flags (none defined in v1).
    #[allow(dead_code, reason = "no v1 flags; kept for the frozen header layout")]
    pub(crate) fn flags(&self) -> u16 {
        self.flags
    }

    /// Iterates the TLV fields of the payload.
    pub(crate) fn tlvs(&self) -> TlvIter<'_> {
        TlvIter::new(&self.payload)
    }
}

/// One TLV field borrowed from a frame payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Tlv<'a> {
    tag: u16,
    value: &'a [u8],
}

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
pub(crate) struct TlvIter<'a> {
    rest: &'a [u8],
}

impl<'a> TlvIter<'a> {
    /// Iterates the TLV fields in `bytes` (a payload or a nested TLV value).
    pub(crate) fn new(bytes: &'a [u8]) -> Self {
        Self { rest: bytes }
    }

    fn split(&self) -> Option<(Tlv<'a>, &'a [u8])> {
        let (tag, rest) = self.rest.split_first_chunk::<2>()?;
        let (len, rest) = rest.split_first_chunk::<4>()?;
        let len = usize::try_from(u32::from_be_bytes(*len)).ok()?;
        let (value, rest) = rest.split_at_checked(len)?;
        let tag = u16::from_be_bytes(*tag);
        Some((Tlv { tag, value }, rest))
    }
}

impl<'a> Iterator for TlvIter<'a> {
    type Item = Result<Tlv<'a>, FrameError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.rest.is_empty() {
            return None;
        }
        match self.split() {
            Some((tlv, rest)) => {
                self.rest = rest;
                Some(Ok(tlv))
            }
            None => {
                self.rest = &[];
                Some(Err(FrameError::Truncated))
            }
        }
    }
}

/// Appends one TLV field.
pub(crate) fn put_tlv(out: &mut Vec<u8>, tag: u16, value: &[u8]) {
    out.reserve(TLV_HEADER_LEN.saturating_add(value.len()));
    out.extend_from_slice(&tag.to_be_bytes());
    out.extend_from_slice(&wire_len(value.len()).to_be_bytes());
    out.extend_from_slice(value);
}

/// Appends one frame with `payload`.
pub(crate) fn put_frame(out: &mut Vec<u8>, msg_type: u16, payload: &[u8]) {
    out.reserve(HEADER_LEN.saturating_add(payload.len()));
    out.extend_from_slice(&wire_len(payload.len()).to_be_bytes());
    out.extend_from_slice(&msg_type.to_be_bytes());
    out.extend_from_slice(&0u16.to_be_bytes());
    out.extend_from_slice(payload);
}

/// A length as `u32`. Saturates: the peer's decoder then rejects it as `TooLong`.
fn wire_len(len: usize) -> u32 {
    u32::try_from(len).unwrap_or(u32::MAX)
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

impl std::fmt::Display for FrameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingTag(t) | Self::BadValue(t) => write!(f, "{} (tag {t:#06x})", self.code()),
            Self::TooLong | Self::Truncated => f.write_str(self.code()),
        }
    }
}

impl std::error::Error for FrameError {}

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
    pub fn next_frame(&mut self) -> Result<Option<Frame>, FrameError> {
        let Some((len, rest)) = self.buf.split_first_chunk::<4>() else {
            return Ok(None);
        };
        let Some((msg_type, rest)) = rest.split_first_chunk::<2>() else {
            return Ok(None);
        };
        let Some((flags, rest)) = rest.split_first_chunk::<2>() else {
            return Ok(None);
        };
        let len = usize::try_from(u32::from_be_bytes(*len)).map_err(|_| FrameError::TooLong)?;
        if len > MAX_FRAME_LEN {
            return Err(FrameError::TooLong);
        }
        let Some(payload) = rest.get(..len) else {
            return Ok(None);
        };
        let frame = Frame {
            msg_type: MsgType::from_raw(u16::from_be_bytes(*msg_type)),
            flags: u16::from_be_bytes(*flags),
            payload: payload.to_vec(),
        };
        self.buf.drain(..HEADER_LEN.saturating_add(len));
        Ok(Some(frame))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decoder_yields_none_until_a_frame_is_complete() {
        let mut dec = FrameDecoder::new();
        dec.push(&[0, 0, 0]);
        assert_eq!(dec.next_frame(), Ok(None));
        dec.push(&[2, 0, 4, 0, 0, 0xAA]);
        assert_eq!(dec.next_frame(), Ok(None));
        dec.push(&[0xBB, 0xCC]);
        let frame = dec.next_frame().unwrap().unwrap();
        assert_eq!(
            (frame.msg_type().raw(), frame.payload.as_slice()),
            (4, &[0xAA, 0xBB][..])
        );
        assert_eq!(dec.next_frame(), Ok(None));
        assert_eq!(dec.buf, vec![0xCC]);
    }

    #[test]
    fn decoder_rejects_a_frame_above_one_mib() {
        let mut dec = FrameDecoder::new();
        dec.push(&[0, 0x10, 0, 1, 0, 1, 0, 0]);
        assert_eq!(dec.next_frame(), Err(FrameError::TooLong));
    }

    #[test]
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

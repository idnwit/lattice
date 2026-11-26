//! The 64-byte ed25519 signature type and its serde form.

use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

/// Length of a [`Signature`] on the wire, in bytes.
const SIGNATURE_LEN: usize = 64;

/// An ed25519 signature. Serde form: 64 raw bytes.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Signature([u8; SIGNATURE_LEN]);

impl Signature {
    /// Wraps 64 signature bytes read from the wire. Checked on verify.
    pub fn from_bytes(bytes: [u8; SIGNATURE_LEN]) -> Self {
        Self(bytes)
    }

    /// The 64 signature bytes, as sent on the wire (e.g. a TLS `CertificateVerify`).
    pub fn to_bytes(&self) -> [u8; SIGNATURE_LEN] {
        self.0
    }

    pub(crate) fn from_dalek(sig: &ed25519_dalek::Signature) -> Self {
        Self(sig.to_bytes())
    }

    pub(crate) fn to_dalek(self) -> ed25519_dalek::Signature {
        ed25519_dalek::Signature::from_bytes(&self.0)
    }
}

impl fmt::Debug for Signature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Signature(..)")
    }
}

impl Serialize for Signature {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_bytes(&self.0)
    }
}

impl<'de> Deserialize<'de> for Signature {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        d.deserialize_bytes(SignatureVisitor)
    }
}

struct SignatureVisitor;

impl Visitor<'_> for SignatureVisitor {
    type Value = Signature;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{SIGNATURE_LEN} signature bytes")
    }

    fn visit_bytes<E: de::Error>(self, v: &[u8]) -> Result<Signature, E> {
        let bytes =
            <[u8; SIGNATURE_LEN]>::try_from(v).map_err(|_| E::invalid_length(v.len(), &self))?;
        Ok(Signature(bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signature_roundtrips_through_postcard() {
        let sig = Signature::from_bytes([5; SIGNATURE_LEN]);
        let bytes = postcard::to_allocvec(&sig).unwrap();
        assert_eq!(postcard::from_bytes::<Signature>(&bytes).unwrap(), sig);
    }

    #[test]
    fn signature_rejects_wrong_length() {
        // postcard byte string: varint length 10, then 10 bytes.
        let mut bytes = vec![10u8];
        bytes.extend_from_slice(&[1; 10]);
        assert!(postcard::from_bytes::<Signature>(&bytes).is_err());
    }
}

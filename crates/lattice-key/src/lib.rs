//! Node identity: the ed25519 key every worker and deck holds.
//!
//! A [`NodeId`] is a node's ed25519 public key. It is also its transport
//! address (`docs/architecture/03-identity-security.md`). A [`NodeKey`] is the
//! secret half; it signs relay registrations and sync changes.
//!
//! This crate owns key types only. It does no I/O: callers load secret bytes
//! from disk or the Keychain and generate them from the OS RNG.
//! The device list, content keys, and pairing live in `lattice-identity`
//! (not yet created).
//!
//! # Usage
//!
//! ```
//! use lattice_key::{NodeKey, SecretKeyBytes};
//!
//! let key = NodeKey::from_secret(SecretKeyBytes::new([7; 32]));
//! let sig = key.sign(b"hello");
//! assert!(key.node_id().verify(b"hello", &sig).is_ok());
//! ```
//!
//! # Key items
//!
//! - [`NodeId`]: public key, `Copy`, ordered, serde as 32 raw bytes.
//! - [`NodeKey`]: signing key with a redacting `Debug`.
//! - [`Signature`]: 64-byte ed25519 signature.
//! - [`KeyError`]: invalid key or signature, with a stable [`KeyError::code`].

#![deny(clippy::indexing_slicing, clippy::arithmetic_side_effects)]
#![deny(clippy::cast_possible_truncation)]

mod signature;

pub use signature::Signature;

use ed25519_dalek::{SigningKey, VerifyingKey};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

/// Length of a [`NodeId`] on the wire, in bytes.
pub const NODE_ID_LEN: usize = 32;

/// Error from parsing a key or checking a signature.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum KeyError {
    /// The 32 bytes are not a valid ed25519 public key.
    #[error("invalid ed25519 public key")]
    InvalidPublicKey,
    /// The signature does not match the message and key.
    #[error("bad signature")]
    BadSignature,
}

impl KeyError {
    /// Stable code for logs and the wire (`.rules/engineering.md`, Errors).
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidPublicKey => "key.invalid_public_key",
            Self::BadSignature => "key.bad_signature",
        }
    }
}

/// A node's identity: its ed25519 public key.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId([u8; NODE_ID_LEN]);

impl NodeId {
    /// Parses 32 bytes into a node id.
    ///
    /// # Errors
    ///
    /// [`KeyError::InvalidPublicKey`] when the bytes are not a curve point.
    pub fn try_from_bytes(bytes: [u8; NODE_ID_LEN]) -> Result<Self, KeyError> {
        VerifyingKey::from_bytes(&bytes).map_err(|_| KeyError::InvalidPublicKey)?;
        Ok(Self(bytes))
    }

    /// The raw 32 bytes, as sent on the wire.
    pub fn as_bytes(&self) -> &[u8; NODE_ID_LEN] {
        &self.0
    }

    /// Checks `sig` over `msg` with this key (strict verification).
    ///
    /// # Errors
    ///
    /// [`KeyError::BadSignature`] when the signature does not verify.
    pub fn verify(&self, msg: &[u8], sig: &Signature) -> Result<(), KeyError> {
        let key = VerifyingKey::from_bytes(&self.0).map_err(|_| KeyError::InvalidPublicKey)?;
        key.verify_strict(msg, &sig.to_dalek())
            .map_err(|_| KeyError::BadSignature)
    }
}

impl fmt::Debug for NodeId {
    // Short hex prefix: enough to tell nodes apart in logs.
    // TBD: canonical text form of a NodeId (base32? hex?) for the CLI and pairing codes?
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "NodeId(")?;
        for b in self.0.iter().take(4) {
            write!(f, "{b:02x}")?;
        }
        write!(f, "…)")
    }
}

impl Serialize for NodeId {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(s)
    }
}

impl<'de> Deserialize<'de> for NodeId {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let bytes = <[u8; NODE_ID_LEN]>::deserialize(d)?;
        Self::try_from_bytes(bytes).map_err(serde::de::Error::custom)
    }
}

/// 32 secret bytes of a node key. Never logged.
pub struct SecretKeyBytes([u8; 32]);

impl SecretKeyBytes {
    /// Wraps secret bytes read from storage or the OS RNG.
    pub fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

impl fmt::Debug for SecretKeyBytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretKeyBytes(<redacted>)")
    }
}

/// A node's signing key. `Debug` shows only the public [`NodeId`].
pub struct NodeKey(SigningKey);

impl NodeKey {
    /// Builds the key from its secret bytes.
    pub fn from_secret(secret: SecretKeyBytes) -> Self {
        Self(SigningKey::from_bytes(&secret.0))
    }

    /// The public identity of this key.
    pub fn node_id(&self) -> NodeId {
        NodeId(self.0.verifying_key().to_bytes())
    }

    /// Signs `msg`.
    pub fn sign(&self, msg: &[u8]) -> Signature {
        use ed25519_dalek::Signer as _;
        Signature::from_dalek(&self.0.sign(msg))
    }
}

impl fmt::Debug for NodeKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("NodeKey").field(&self.node_id()).finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(seed: u8) -> NodeKey {
        NodeKey::from_secret(SecretKeyBytes::new([seed; 32]))
    }

    #[test]
    fn signature_verifies_only_for_its_message_and_key() {
        let (a, b) = (key(1), key(2));
        let sig = a.sign(b"msg");
        assert_eq!(a.node_id().verify(b"msg", &sig), Ok(()));
        assert_eq!(
            a.node_id().verify(b"other", &sig),
            Err(KeyError::BadSignature)
        );
        assert_eq!(
            b.node_id().verify(b"msg", &sig),
            Err(KeyError::BadSignature)
        );
    }

    #[test]
    fn node_id_roundtrips_through_postcard() {
        let id = key(3).node_id();
        let bytes = postcard::to_allocvec(&id).unwrap();
        assert_eq!(postcard::from_bytes::<NodeId>(&bytes).unwrap(), id);
    }

    #[test]
    fn node_id_rejects_non_point_bytes() {
        // y = 2 is not on the curve (no valid x).
        let mut bytes = [0u8; NODE_ID_LEN];
        bytes[0] = 2;
        assert_eq!(
            NodeId::try_from_bytes(bytes),
            Err(KeyError::InvalidPublicKey)
        );
    }

    #[test]
    fn error_codes_are_stable_and_namespaced() {
        assert_eq!(KeyError::InvalidPublicKey.code(), "key.invalid_public_key");
        assert_eq!(KeyError::BadSignature.code(), "key.bad_signature");
    }

    #[test]
    fn debug_never_prints_secret_bytes() {
        let shown = format!("{:?} {:?}", SecretKeyBytes::new([9; 32]), key(9));
        assert!(!shown.contains("0909"), "shown={shown}");
    }
}

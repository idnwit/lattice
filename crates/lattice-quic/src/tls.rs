//! TLS 1.3 bound to the node's ed25519 key (01-transport.md, Stack).
//!
//! Each side presents an RFC 7250 raw public key: the DER
//! `SubjectPublicKeyInfo` of its [`NodeId`]. There is no certificate and no CA.
//! Handshake signatures are made with [`NodeKey::sign`] and checked with
//! [`NodeId::verify`], so the secret key never leaves `lattice-key`.
//!
//! - Client: pins the dialed NodeId. Any other key fails the handshake.
//! - Server: requires a client key and accepts any valid ed25519 key. Who may
//!   do what is the host's job (device list), not L1's.

use lattice_key::{NODE_ID_LEN, NodeId, NodeKey, Signature};
use lattice_transport::TransportError;
use quinn::crypto::rustls::{QuicClientConfig, QuicServerConfig};
use quinn::rustls;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, SubjectPublicKeyInfoDer, UnixTime};
use rustls::server::danger::{ClientCertVerified, ClientCertVerifier};
use rustls::sign::{CertifiedKey, Signer, SigningKey};
use rustls::{
    CertificateError, DigitallySignedStruct, DistinguishedName, Error as TlsError,
    SignatureAlgorithm, SignatureScheme,
};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// ALPN of the Lattice node protocol, version 1.
pub(crate) const ALPN: &[u8] = b"lattice/1";

/// Server name sent in the handshake. Unused for auth: the key is pinned.
pub(crate) const SERVER_NAME: &str = "lattice";

/// QUIC keep-alive, so idle terminal sessions stay open.
///
/// TBD: spec value (01-transport.md). Must stay well below [`IDLE_TIMEOUT`].
const KEEP_ALIVE: Duration = Duration::from_secs(5);

/// A connection with no packets for this long is closed.
///
/// TBD: spec value (01-transport.md). quinn's default.
const IDLE_TIMEOUT: Duration = Duration::from_secs(30);

/// Fixed UDP payload on paths that may go through the relay, PMTUD off
/// (01-transport.md, Paths rule 7): 1200 + the 41-byte `SEND` header fits any real path.
const RELAY_SAFE_MTU: u16 = 1200;

/// Which UDP payload size quinn uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mtu {
    /// quinn's defaults with PMTUD: direct-only mode.
    Discover,
    /// [`RELAY_SAFE_MTU`], fixed: relay mode.
    RelaySafe,
}

/// DER prefix of an ed25519 `SubjectPublicKeyInfo` (RFC 8410):
/// `SEQUENCE { SEQUENCE { OID 1.3.101.112 }, BIT STRING (0 unused bits) }`.
const ED25519_SPKI_PREFIX: [u8; 12] = [
    0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x03, 0x21, 0x00,
];

/// Length of an ed25519 signature.
const SIGNATURE_LEN: usize = 64;

/// The DER `SubjectPublicKeyInfo` of `id`.
fn spki(id: NodeId) -> Vec<u8> {
    let mut der = Vec::with_capacity(ED25519_SPKI_PREFIX.len().saturating_add(NODE_ID_LEN));
    der.extend_from_slice(&ED25519_SPKI_PREFIX);
    der.extend_from_slice(id.as_bytes());
    der
}

/// Parses a raw public key; `None` unless it is a valid ed25519 key.
pub(crate) fn node_id_from_spki(der: &[u8]) -> Option<NodeId> {
    let (prefix, key) = der.split_first_chunk::<12>()?;
    if *prefix != ED25519_SPKI_PREFIX {
        return None;
    }
    let key = <[u8; NODE_ID_LEN]>::try_from(key).ok()?;
    NodeId::try_from_bytes(key).ok()
}

/// This node's TLS identity: its raw public key and a signer over [`NodeKey`].
#[derive(Debug, Clone)]
pub(crate) struct Identity {
    certified: Arc<CertifiedKey>,
    mtu: Mtu,
}

impl Identity {
    /// Binds TLS to `key`. Connections use `mtu`.
    pub(crate) fn new(key: Arc<NodeKey>, mtu: Mtu) -> Self {
        let id = key.node_id();
        let signer = Arc::new(NodeSigningKey(key));
        let cert = CertificateDer::from(spki(id));
        Self {
            certified: Arc::new(CertifiedKey::new(vec![cert], signer)),
            mtu,
        }
    }

    /// Server config: present our key, require an ed25519 client key.
    pub(crate) fn server_config(&self) -> Result<quinn::ServerConfig, TransportError> {
        let mut tls = rustls::ServerConfig::builder_with_provider(provider())
            .with_protocol_versions(&[&rustls::version::TLS13])
            .map_err(tls_err)?
            .with_client_cert_verifier(Arc::new(AnyNodeClient))
            .with_cert_resolver(Arc::new(
                rustls::server::AlwaysResolvesServerRawPublicKeys::new(self.certified.clone()),
            ));
        tls.alpn_protocols = vec![ALPN.to_vec()];
        let crypto =
            QuicServerConfig::try_from(tls).map_err(|e| TransportError::Io(e.to_string()))?;
        let mut cfg = quinn::ServerConfig::with_crypto(Arc::new(crypto));
        cfg.transport_config(transport_config(self.mtu)?);
        Ok(cfg)
    }

    /// Client config that accepts only `expected`. Sets `mismatch` when the
    /// server shows another valid key.
    pub(crate) fn client_config(
        &self,
        expected: NodeId,
        mismatch: Arc<AtomicBool>,
    ) -> Result<quinn::ClientConfig, TransportError> {
        let mut tls = rustls::ClientConfig::builder_with_provider(provider())
            .with_protocol_versions(&[&rustls::version::TLS13])
            .map_err(tls_err)?
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(PinnedServer { expected, mismatch }))
            .with_client_cert_resolver(Arc::new(
                rustls::client::AlwaysResolvesClientRawPublicKeys::new(self.certified.clone()),
            ));
        tls.alpn_protocols = vec![ALPN.to_vec()];
        tls.enable_sni = false;
        let crypto =
            QuicClientConfig::try_from(tls).map_err(|e| TransportError::Io(e.to_string()))?;
        let mut cfg = quinn::ClientConfig::new(Arc::new(crypto));
        cfg.transport_config(transport_config(self.mtu)?);
        Ok(cfg)
    }
}

fn provider() -> Arc<rustls::crypto::CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

fn tls_err(e: TlsError) -> TransportError {
    TransportError::Io(e.to_string())
}

/// A random 16-byte nonce from the TLS provider's RNG (relay `REGISTER`).
/// `None` only when the OS has no randomness.
pub(crate) fn random_nonce() -> Option<[u8; 16]> {
    let mut nonce = [0u8; 16];
    provider().secure_random.fill(&mut nonce).ok()?;
    Some(nonce)
}

fn transport_config(mtu: Mtu) -> Result<Arc<quinn::TransportConfig>, TransportError> {
    let idle = quinn::IdleTimeout::try_from(IDLE_TIMEOUT)
        .map_err(|_| TransportError::Io("idle timeout out of range".into()))?;
    let mut t = quinn::TransportConfig::default();
    t.keep_alive_interval(Some(KEEP_ALIVE));
    t.max_idle_timeout(Some(idle));
    if mtu == Mtu::RelaySafe {
        t.initial_mtu(RELAY_SAFE_MTU);
        t.min_mtu(RELAY_SAFE_MTU);
        t.mtu_discovery_config(None);
    }
    Ok(Arc::new(t))
}

/// Checks a TLS 1.3 `CertificateVerify` made by the raw key in `cert`.
fn verify_ed25519(
    message: &[u8],
    cert: &CertificateDer<'_>,
    dss: &DigitallySignedStruct,
) -> Result<HandshakeSignatureValid, TlsError> {
    check_ed25519(message, cert, dss.scheme, dss.signature())
        .map(|()| HandshakeSignatureValid::assertion())
}

fn check_ed25519(
    message: &[u8],
    spki_der: &[u8],
    scheme: SignatureScheme,
    signature: &[u8],
) -> Result<(), TlsError> {
    let bad = || TlsError::InvalidCertificate(CertificateError::BadSignature);
    if scheme != SignatureScheme::ED25519 {
        return Err(bad());
    }
    let id = node_id_from_spki(spki_der)
        .ok_or(TlsError::InvalidCertificate(CertificateError::BadEncoding))?;
    let sig = <[u8; SIGNATURE_LEN]>::try_from(signature).map_err(|_| bad())?;
    id.verify(message, &Signature::from_bytes(sig))
        .map_err(|_| bad())
}

fn tls12_refused() -> TlsError {
    TlsError::General("TLS 1.2 is not supported".into())
}

/// Server verifier on the dialing side: the key must equal the dialed NodeId.
#[derive(Debug)]
struct PinnedServer {
    expected: NodeId,
    mismatch: Arc<AtomicBool>,
}

impl ServerCertVerifier for PinnedServer {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, TlsError> {
        let id = intermediates
            .is_empty()
            .then(|| node_id_from_spki(end_entity))
            .flatten()
            .ok_or(TlsError::InvalidCertificate(CertificateError::BadEncoding))?;
        if id == self.expected {
            return Ok(ServerCertVerified::assertion());
        }
        self.mismatch.store(true, Ordering::SeqCst);
        Err(TlsError::InvalidCertificate(
            CertificateError::ApplicationVerificationFailure,
        ))
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        Err(tls12_refused())
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        verify_ed25519(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![SignatureScheme::ED25519]
    }

    fn requires_raw_public_keys(&self) -> bool {
        true
    }
}

/// Client verifier on the accepting side: any valid ed25519 key, mandatory.
#[derive(Debug)]
struct AnyNodeClient;

impl ClientCertVerifier for AnyNodeClient {
    fn client_auth_mandatory(&self) -> bool {
        true
    }

    fn root_hint_subjects(&self) -> &[DistinguishedName] {
        &[]
    }

    fn verify_client_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        _now: UnixTime,
    ) -> Result<ClientCertVerified, TlsError> {
        match (intermediates.is_empty(), node_id_from_spki(end_entity)) {
            (true, Some(_)) => Ok(ClientCertVerified::assertion()),
            _ => Err(TlsError::InvalidCertificate(CertificateError::BadEncoding)),
        }
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        Err(tls12_refused())
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        verify_ed25519(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![SignatureScheme::ED25519]
    }

    fn requires_raw_public_keys(&self) -> bool {
        true
    }
}

/// rustls signing key backed by [`NodeKey`]. `Debug` shows only the NodeId.
#[derive(Debug)]
struct NodeSigningKey(Arc<NodeKey>);

impl SigningKey for NodeSigningKey {
    fn choose_scheme(&self, offered: &[SignatureScheme]) -> Option<Box<dyn Signer>> {
        offered
            .contains(&SignatureScheme::ED25519)
            .then(|| Box::new(NodeSigner(self.0.clone())) as Box<dyn Signer>)
    }

    fn public_key(&self) -> Option<SubjectPublicKeyInfoDer<'_>> {
        Some(SubjectPublicKeyInfoDer::from(spki(self.0.node_id())))
    }

    fn algorithm(&self) -> SignatureAlgorithm {
        SignatureAlgorithm::ED25519
    }
}

#[derive(Debug)]
struct NodeSigner(Arc<NodeKey>);

impl Signer for NodeSigner {
    fn sign(&self, message: &[u8]) -> Result<Vec<u8>, TlsError> {
        Ok(self.0.sign(message).to_bytes().to_vec())
    }

    fn scheme(&self) -> SignatureScheme {
        SignatureScheme::ED25519
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lattice_key::SecretKeyBytes;

    fn id(seed: u8) -> NodeId {
        NodeKey::from_secret(SecretKeyBytes::new([seed; 32])).node_id()
    }

    #[test]
    fn spki_roundtrips_to_the_node_id() {
        let der = spki(id(1));
        assert_eq!(der.len(), 44);
        assert_eq!(node_id_from_spki(&der), Some(id(1)));
    }

    #[test]
    fn non_ed25519_keys_are_rejected() {
        let mut wrong_oid = spki(id(1));
        wrong_oid[8] = 0x71; // 1.3.101.113 = ed448
        let cases: [(&str, &[u8]); 3] = [
            ("empty", &[]),
            ("truncated", &spki(id(1))[..43]),
            ("ed448 oid", &wrong_oid),
        ];
        for (name, der) in cases {
            assert_eq!(node_id_from_spki(der), None, "case={name}");
        }
    }

    #[test]
    fn handshake_signature_verifies_only_for_its_key() {
        let key = NodeKey::from_secret(SecretKeyBytes::new([1; 32]));
        let sig = NodeSigner(Arc::new(key)).sign(b"transcript").unwrap();
        let ed = SignatureScheme::ED25519;
        let (own, other) = (spki(id(1)), spki(id(2)));
        assert_eq!(check_ed25519(b"transcript", &own, ed, &sig), Ok(()));
        assert!(check_ed25519(b"other", &own, ed, &sig).is_err());
        assert!(check_ed25519(b"transcript", &other, ed, &sig).is_err());
        let p256 = SignatureScheme::ECDSA_NISTP256_SHA256;
        assert!(check_ed25519(b"transcript", &own, p256, &sig).is_err());
    }
}

//! Checked field reads for `decode`.

use super::{CodecError, ObservedAddr, Token};
use lattice_key::NodeId;
use std::net::{IpAddr, Ipv6Addr, SocketAddr};

/// Checked reads over the fields after the type byte.
pub(super) struct Reader<'a>(pub(super) &'a [u8]);

impl Reader<'_> {
    pub(super) fn array<const N: usize>(&mut self) -> Result<[u8; N], CodecError> {
        let (head, rest) = self
            .0
            .split_first_chunk::<N>()
            .ok_or(CodecError::Truncated)?;
        self.0 = rest;
        Ok(*head)
    }

    pub(super) fn node_id(&mut self) -> Result<NodeId, CodecError> {
        NodeId::try_from_bytes(self.array()?).map_err(|_| CodecError::BadNodeId)
    }

    pub(super) fn token(&mut self) -> Result<Token, CodecError> {
        self.array().map(Token::from_bytes)
    }

    pub(super) fn addr(&mut self) -> Result<ObservedAddr, CodecError> {
        let ip = Ipv6Addr::from(self.array::<16>()?);
        let port = u16::from_be_bytes(self.array()?);
        Ok(ObservedAddr::new(SocketAddr::new(IpAddr::V6(ip), port)))
    }
}

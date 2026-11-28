//! The home relay URL and the relay id pin (specs/10-settings.md, rules 29, 37–39;
//! specs/02-connectivity.md, rule 21).

use std::fmt;
use std::str::FromStr;

use lattice_key::NodeId;

use crate::host::{HostError, HostPort};

/// URL scheme of the udp relay.
const SCHEME: &str = "udp://";
/// Text of the "no relay" value.
const NONE: &str = "none";

/// The value of `relay.addr`: `udp://[<relay id>@]<host>:<port>` or `none`.
///
/// The host is canonical (specs/01-pairing.md, Contact). The relay id is 64 hex digits;
/// it prints in lower case.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelayAddr {
    /// Direct only: no home relay.
    None,
    /// A home relay.
    Relay {
        /// Explicit pin from the URL; always wins (rule 38).
        id: Option<NodeId>,
        /// Where the relay listens.
        at: HostPort,
    },
}

impl FromStr for RelayAddr {
    type Err = HostError;

    fn from_str(s: &str) -> Result<Self, HostError> {
        if s == NONE {
            return Ok(Self::None);
        }
        let rest = s.strip_prefix(SCHEME).ok_or(HostError)?;
        let (id, at) = match rest.split_once('@') {
            Some((id, at)) => (Some(id.parse::<NodeId>().map_err(|_| HostError)?), at),
            None => (None, rest),
        };
        Ok(Self::Relay {
            id,
            at: at.parse()?,
        })
    }
}

impl fmt::Display for RelayAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::None => f.write_str(NONE),
            Self::Relay { id: Some(id), at } => write!(f, "{SCHEME}{id}@{at}"),
            Self::Relay { id: None, at } => write!(f, "{SCHEME}{at}"),
        }
    }
}

/// Where the relay id used to register comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelayId {
    /// The id inside the URL (rule 38).
    Explicit(NodeId),
    /// `relay.id`, pinned for this relay's `host:port` (rule 37).
    Pinned(NodeId),
    /// No id yet: the first contact pins one (rule 39; lands with ROADMAP M2.8).
    Unpinned,
}

impl RelayId {
    /// The id to register with, when one is known.
    pub fn known(self) -> Option<NodeId> {
        match self {
            Self::Explicit(id) | Self::Pinned(id) => Some(id),
            Self::Unpinned => None,
        }
    }
}

/// The resolved home relay: its address and the id to register with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelayPlan {
    /// `relay.addr = none`.
    None,
    /// Register at `at`.
    Relay {
        /// Where the relay listens.
        at: HostPort,
        /// The relay id and where it came from.
        id: RelayId,
    },
}

impl RelayPlan {
    /// Combines the resolved `relay.addr` with the stored pin.
    ///
    /// The pin counts only when `pinned_for` equals the URL's `host:port`; a pin for
    /// another relay is ignored (rules 29 and 37). An id in the URL always wins (rule 38).
    pub fn new(addr: &RelayAddr, pin: Option<&Pin>) -> Self {
        let RelayAddr::Relay { id, at } = addr else {
            return Self::None;
        };
        let id = match (id, pin) {
            (Some(id), _) => RelayId::Explicit(*id),
            (None, Some(pin)) if pin.pinned_for == *at => RelayId::Pinned(pin.id),
            (None, _) => RelayId::Unpinned,
        };
        Self::Relay { at: at.clone(), id }
    }

    /// The pin to write after the first contact showed `observed` (rule 39).
    ///
    /// `None` unless the plan is [`RelayId::Unpinned`]: an explicit id is never
    /// overwritten (rule 38), and an existing pin is never replaced by a first contact
    /// (specs/02-connectivity.md, rule 22). The magic socket learns the id from `COOKIE`
    /// in ROADMAP M2.8; until then nothing calls this with a live id.
    pub fn pin_on_first_contact(&self, observed: NodeId) -> Option<Pin> {
        match self {
            Self::Relay {
                at,
                id: RelayId::Unpinned,
            } => Some(Pin {
                id: observed,
                pinned_for: at.clone(),
            }),
            _ => None,
        }
    }
}

/// A relay id pin: `relay.id` plus `relay.pinned_for` (rule 37).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pin {
    id: NodeId,
    pinned_for: HostPort,
}

impl Pin {
    /// A pin of `id` for the relay at `pinned_for`.
    pub fn new(id: NodeId, pinned_for: HostPort) -> Self {
        Self { id, pinned_for }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(seed: u8) -> NodeId {
        lattice_key::NodeKey::from_secret(lattice_key::SecretKeyBytes::new([seed; 32])).node_id()
    }

    fn at(s: &str) -> HostPort {
        s.parse().unwrap()
    }

    #[test]
    fn relay_urls_roundtrip_in_canonical_form() {
        let with_id = format!("udp://{}@[2001:db8::1]:7400", id(1));
        for case in ["none", "udp://relay.example.com:7400", with_id.as_str()] {
            let addr: RelayAddr = case.parse().unwrap_or_else(|_| panic!("case={case}"));
            assert_eq!(addr.to_string(), case, "case={case}");
        }
        let upper = format!("udp://{}@10.0.0.1:7400", id(1).to_string().to_uppercase());
        let addr: RelayAddr = upper.parse().unwrap();
        assert_eq!(
            addr.to_string(),
            upper.to_lowercase(),
            "ids print lower case"
        );
    }

    #[test]
    fn relay_urls_with_a_non_canonical_host_fail() {
        for case in [
            "None",
            "relay.example.com:7400",
            "tcp://relay.example.com:7400",
            "udp://Relay.example.com:7400",
            "udp://relay.example.com.:7400",
            "udp://relay.example.com",
            "udp://abc@relay.example.com:7400",
            "udp://2001:db8::1:7400",
        ] {
            assert_eq!(case.parse::<RelayAddr>(), Err(HostError), "case={case}");
        }
    }

    /// specs/10-settings.md, Invariant 1.
    #[test]
    fn a_pin_for_another_host_is_ignored() {
        let url: RelayAddr = "udp://relay.example.com:7400".parse().unwrap();
        let other = Pin::new(id(1), at("other.example.com:7400"));
        let other_port = Pin::new(id(1), at("relay.example.com:7401"));
        let same = Pin::new(id(2), at("relay.example.com:7400"));
        let plan = |pin: Option<&Pin>| match RelayPlan::new(&url, pin) {
            RelayPlan::Relay { id, .. } => id,
            RelayPlan::None => RelayId::Unpinned,
        };
        assert_eq!(plan(Some(&other)), RelayId::Unpinned, "another host");
        assert_eq!(plan(Some(&other_port)), RelayId::Unpinned, "another port");
        assert_eq!(plan(None), RelayId::Unpinned, "no pin");
        assert_eq!(plan(Some(&same)), RelayId::Pinned(id(2)), "this relay");
        let none = RelayPlan::new(&RelayAddr::None, Some(&same));
        assert_eq!(none, RelayPlan::None, "relay none ignores every pin");
    }

    /// specs/10-settings.md, Invariant 2.
    #[test]
    fn an_explicit_id_is_never_overwritten() {
        let url: RelayAddr = format!("udp://{}@relay.example.com:7400", id(1))
            .parse()
            .unwrap();
        let pin = Pin::new(id(2), at("relay.example.com:7400"));
        for stored in [None, Some(&pin)] {
            let plan = RelayPlan::new(&url, stored);
            let RelayPlan::Relay { id: got, .. } = &plan else {
                panic!("a relay plan");
            };
            assert_eq!(*got, RelayId::Explicit(id(1)), "the URL id wins");
            assert_eq!(plan.pin_on_first_contact(id(3)), None, "never re-pinned");
        }
    }

    #[test]
    fn only_an_unpinned_relay_pins_on_first_contact() {
        let url: RelayAddr = "udp://relay.example.com:7400".parse().unwrap();
        let unpinned = RelayPlan::new(&url, None);
        let want = Pin::new(id(3), at("relay.example.com:7400"));
        assert_eq!(unpinned.pin_on_first_contact(id(3)), Some(want));
        let pinned = RelayPlan::new(&url, Some(&Pin::new(id(2), at("relay.example.com:7400"))));
        assert_eq!(pinned.pin_on_first_contact(id(3)), None, "keeps its pin");
        assert_eq!(RelayPlan::None.pin_on_first_contact(id(3)), None);
    }
}

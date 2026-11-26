//! State values (design-system tokens.md, State). Views map each to shape + hue.

/// State of a worker, pane, tab, or session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Worker reachable, direct P2P.
    Online,
    /// Reachable via relay.
    Relay,
    /// Agent running.
    Agent,
    /// Needs input or approval.
    Waiting,
    /// Exit ≠ 0, link lost.
    Error,
    /// Worker asleep.
    Offline,
    /// Agent idle.
    Idle,
}

impl Status {
    /// The glyph a tab or session shows: its busiest child.
    ///
    /// Priority: waiting > error > agent > online/relay > idle > offline
    /// (tokens.md; TBD there: confirm the order). `None` for no children.
    pub fn busiest(children: impl IntoIterator<Item = Status>) -> Option<Status> {
        let mut best: Option<Status> = None;
        for s in children {
            if best.is_none_or(|b| s.rank() > b.rank()) {
                best = Some(s);
            }
        }
        best
    }

    /// Priority rank; higher wins.
    fn rank(self) -> u8 {
        match self {
            Self::Waiting => 5,
            Self::Error => 4,
            Self::Agent => 3,
            Self::Online | Self::Relay => 2,
            Self::Idle => 1,
            Self::Offline => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Status::*;
    use super::*;

    #[test]
    fn busiest_child_follows_the_token_priority() {
        let cases = [
            (vec![], None),
            (vec![Idle, Waiting, Error], Some(Waiting)),
            (vec![Agent, Error], Some(Error)),
            (vec![Online, Agent], Some(Agent)),
            (vec![Offline, Idle], Some(Idle)),
        ];
        for (input, want) in cases {
            assert_eq!(Status::busiest(input.clone()), want, "case={input:?}");
        }
    }
}

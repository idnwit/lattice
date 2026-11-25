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
    #[allow(
        clippy::todo,
        unused_variables,
        reason = "L1 skeleton: body lands with its contract test"
    )]
    pub fn busiest(children: impl IntoIterator<Item = Status>) -> Option<Status> {
        // rank each status by the priority list; return the max (first wins on ties)
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::Status::*;
    use super::*;

    #[test]
    #[ignore = "contract: lands with Status::busiest"]
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

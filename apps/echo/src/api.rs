//! Params, reducers, and errors of the echo app.

use lattice_app::{App, Ctx};
use serde::{Deserialize, Serialize};

use crate::model::EchoState;

/// Longest text `send` accepts, in UTF-8 bytes (4 KiB).
pub const MAX_TEXT_BYTES: usize = 4096;

/// Params of `create`: none. Encodes to zero bytes.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EchoParams {}

/// Request of `send`: one line of text.
///
/// Named `Message`, not `Send`, so it never shadows the `Send` marker trait where imported.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    text: String,
}

impl Message {
    /// A message with this text. Checked by the runner, not here.
    pub fn new(text: impl Into<String>) -> Self {
        Self { text: text.into() }
    }

    /// The text.
    pub fn text(&self) -> &str {
        &self.text
    }

    #[cfg(feature = "runner")]
    pub(crate) fn into_text(self) -> String {
        self.text
    }
}

/// Reducers of the echo app: the contract between runner and viewer (06-apps.md, Interface).
///
/// Append new methods at the end: the action tag is the declaration order.
#[lattice_app::interface]
pub trait EchoApi: App<State = EchoState, Params = EchoParams, Error = EchoError> {
    /// Appends a line. Returns its sequence number.
    fn send(
        &mut self,
        ctx: &mut Ctx,
        state: &mut EchoState,
        req: Message,
    ) -> Result<u64, EchoError>;

    /// Removes every line. Sequence numbers keep counting.
    fn clear(&mut self, ctx: &mut Ctx, state: &mut EchoState, req: ()) -> Result<(), EchoError>;
}

/// Errors of the echo app. Variant names are wire codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[serde(rename_all = "snake_case")]
pub enum EchoError {
    /// The text is empty or only whitespace.
    #[error("empty text")]
    Empty,
    /// The text is longer than [`MAX_TEXT_BYTES`].
    #[error("text longer than 4 KiB")]
    TooLong,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_message_keeps_its_text() {
        assert_eq!(Message::new("hi").text(), "hi");
    }
}

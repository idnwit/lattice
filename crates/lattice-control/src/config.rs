//! `worker/config.toml`: keys, validation, and the file text (specs/10-settings.md,
//! Worker config).

use std::fmt;
use std::net::SocketAddr;
use std::str::FromStr;

use lattice_key::NodeId;

use crate::host::HostPort;
use crate::relay::{Pin, RelayAddr};

/// A worker config key (specs/10-settings.md, Worker config table).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Key {
    /// `relay.addr`: home relay URL or `none`.
    RelayAddr,
    /// `relay.id`: pinned relay id, 64 hex digits.
    RelayId,
    /// `relay.pinned_for`: relay `host:port` that `relay.id` belongs to.
    RelayPinnedFor,
    /// `listen.addr`: UDP listen address.
    ListenAddr,
    /// `name`: worker name.
    Name,
}

impl Key {
    /// Every key, in the order `lattice config list` prints them.
    pub const ALL: [Self; 5] = [
        Self::RelayAddr,
        Self::RelayId,
        Self::RelayPinnedFor,
        Self::ListenAddr,
        Self::Name,
    ];

    /// The dotted key name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::RelayAddr => "relay.addr",
            Self::RelayId => "relay.id",
            Self::RelayPinnedFor => "relay.pinned_for",
            Self::ListenAddr => "listen.addr",
            Self::Name => "name",
        }
    }
}

impl FromStr for Key {
    type Err = ConfigError;

    fn from_str(s: &str) -> Result<Self, ConfigError> {
        Self::ALL
            .into_iter()
            .find(|k| k.as_str() == s)
            .ok_or(ConfigError::UnknownKey)
    }
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A config file, value, or key was refused (specs/10-settings.md, rule 46).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ConfigError {
    /// The text is not TOML.
    #[error("config.parse: {0}")]
    Parse(String),
    /// A key without a schema.
    #[error("config.unknown_key")]
    UnknownKey,
    /// A value that fails its key's rule.
    #[error("config.invalid_value: {0}")]
    InvalidValue(Key),
    /// `ConfigReplace`: the file no longer has `base_hash` (rule 45).
    #[error("config.conflict")]
    Conflict,
    /// Reading or writing the file failed. TBD: the spec names no code for this; which?
    #[error("config.io: {0}")]
    Io(String),
}

impl ConfigError {
    /// Stable code for the CLI, logs and `Failed{code}`.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Parse(_) => "config.parse",
            Self::UnknownKey => "config.unknown_key",
            Self::InvalidValue(_) => "config.invalid_value",
            Self::Conflict => "config.conflict",
            Self::Io(_) => "config.io",
        }
    }
}

/// The keys one layer sets: the file, the environment, or the flags. Every value is
/// valid; an absent key falls to the next layer (rule 33).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConfigFile {
    relay_addr: Option<RelayAddr>,
    relay_id: Option<NodeId>,
    pinned_for: Option<HostPort>,
    listen_addr: Option<SocketAddr>,
    name: Option<String>,
}

impl ConfigFile {
    /// Parses `config.toml` text. Empty text is a file that sets nothing.
    ///
    /// # Errors
    ///
    /// [`ConfigError::Parse`] for text that is not TOML, [`ConfigError::UnknownKey`] for
    /// any key outside the table, [`ConfigError::InvalidValue`] for a value of the wrong
    /// type or shape.
    pub fn parse(text: &str) -> Result<Self, ConfigError> {
        let table: toml::Table =
            toml::from_str(text).map_err(|e| ConfigError::Parse(e.message().to_owned()))?;
        let mut file = Self::default();
        for (name, value) in &table {
            match (name.as_str(), value) {
                ("relay" | "listen", toml::Value::Table(inner)) => {
                    for (sub, value) in inner {
                        file.set_toml(&format!("{name}.{sub}"), value)?;
                    }
                }
                _ => file.set_toml(name, value)?,
            }
        }
        Ok(file)
    }

    fn set_toml(&mut self, name: &str, value: &toml::Value) -> Result<(), ConfigError> {
        let key: Key = name.parse()?;
        let text = value.as_str().ok_or(ConfigError::InvalidValue(key))?;
        self.set(key, text)
    }

    /// Validates `value` and stores it under `key`.
    ///
    /// # Errors
    ///
    /// [`ConfigError::InvalidValue`] when `value` fails the key's rule; `self` is unchanged.
    pub fn set(&mut self, key: Key, value: &str) -> Result<(), ConfigError> {
        let bad = ConfigError::InvalidValue(key);
        match key {
            Key::RelayAddr => self.relay_addr = Some(value.parse().map_err(|_| bad)?),
            Key::RelayId => self.relay_id = Some(value.parse().map_err(|_| bad)?),
            Key::RelayPinnedFor => self.pinned_for = Some(value.parse().map_err(|_| bad)?),
            Key::ListenAddr => self.listen_addr = Some(value.parse().map_err(|_| bad)?),
            Key::Name => self.name = Some(valid_name(value).ok_or(bad)?),
        }
        Ok(())
    }

    /// Removes `key`. Removing `relay.id` also removes `relay.pinned_for` (rule 44).
    pub fn reset(&mut self, key: Key) {
        match key {
            Key::RelayAddr => self.relay_addr = None,
            Key::RelayId => {
                self.relay_id = None;
                self.pinned_for = None;
            }
            Key::RelayPinnedFor => self.pinned_for = None,
            Key::ListenAddr => self.listen_addr = None,
            Key::Name => self.name = None,
        }
    }

    /// The value of `key` in text form, when this layer sets it.
    pub fn get(&self, key: Key) -> Option<String> {
        match key {
            Key::RelayAddr => self.relay_addr.as_ref().map(ToString::to_string),
            Key::RelayId => self.relay_id.as_ref().map(ToString::to_string),
            Key::RelayPinnedFor => self.pinned_for.as_ref().map(ToString::to_string),
            Key::ListenAddr => self.listen_addr.as_ref().map(ToString::to_string),
            Key::Name => self.name.clone(),
        }
    }

    pub(crate) fn relay_addr(&self) -> Option<&RelayAddr> {
        self.relay_addr.as_ref()
    }

    pub(crate) fn listen_addr(&self) -> Option<SocketAddr> {
        self.listen_addr
    }

    pub(crate) fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// The stored pin, when both halves are set.
    pub(crate) fn pin(&self) -> Option<Pin> {
        Some(Pin::new(self.relay_id?, self.pinned_for.clone()?))
    }

    /// The file text: `name` first, then the `[relay]` and `[listen]` tables.
    pub fn to_toml(&self) -> String {
        let mut out = String::new();
        if let Some(name) = &self.name {
            line(&mut out, "name", name);
        }
        for (table, keys) in [
            (
                "relay",
                &[Key::RelayAddr, Key::RelayId, Key::RelayPinnedFor][..],
            ),
            ("listen", &[Key::ListenAddr][..]),
        ] {
            let set: Vec<(Key, String)> = keys
                .iter()
                .filter_map(|k| Some((*k, self.get(*k)?)))
                .collect();
            if set.is_empty() {
                continue;
            }
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(&format!("[{table}]\n"));
            for (key, value) in set {
                let short = key.as_str().rsplit('.').next().unwrap_or(key.as_str());
                line(&mut out, short, &value);
            }
        }
        out
    }
}

/// `key = "value"` with TOML basic-string escapes.
fn line(out: &mut String, key: &str, value: &str) {
    out.push_str(key);
    out.push_str(" = \"");
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            c if c.is_control() => out.push_str(&format!("\\u{:04X}", u32::from(c))),
            c => out.push(c),
        }
    }
    out.push_str("\"\n");
}

/// A worker name: not empty, no control characters.
///
/// TBD: the spec sets no rule for `name` (length, characters). Which one?
fn valid_name(value: &str) -> Option<String> {
    let ok = !value.trim().is_empty() && !value.chars().any(char::is_control);
    ok.then(|| value.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "3b6a27bcceb6a42d62a3a8d02a6f0d73653215771de243a63ac048a18b59da29";

    #[test]
    fn the_spec_example_parses_and_roundtrips() {
        let text = format!(
            "name = \"build-01\"\n\n[relay]\naddr = \"udp://relay.example.com:7400\"\n\
             id = \"{ID}\"\npinned_for = \"relay.example.com:7400\"\n\n[listen]\naddr = \"0.0.0.0:7441\"\n"
        );
        let file = ConfigFile::parse(&text).unwrap();
        assert_eq!(file.get(Key::Name).as_deref(), Some("build-01"));
        assert_eq!(file.get(Key::RelayId).as_deref(), Some(ID));
        assert_eq!(file.to_toml(), text);
        assert_eq!(ConfigFile::parse(&file.to_toml()).unwrap(), file);
    }

    #[test]
    fn an_empty_file_sets_nothing() {
        let file = ConfigFile::parse("").unwrap();
        assert_eq!(file, ConfigFile::default());
        assert_eq!(file.to_toml(), "");
    }

    #[test]
    fn bad_files_fail_with_their_code() {
        let cases = [
            ("name = ", "config.parse"),
            ("color = \"red\"", "config.unknown_key"),
            ("[relay]\nport = \"1\"", "config.unknown_key"),
            ("[gui]\naddr = \"1\"", "config.unknown_key"),
            ("relay = \"none\"", "config.unknown_key"),
            ("name = 3", "config.invalid_value"),
            (
                "[listen]\naddr = \"localhost:7441\"",
                "config.invalid_value",
            ),
            (
                "[relay]\naddr = \"udp://Relay.example.com:7400\"",
                "config.invalid_value",
            ),
            ("[relay]\nid = \"abc\"", "config.invalid_value"),
            (
                "[relay]\npinned_for = \"relay.example.com\"",
                "config.invalid_value",
            ),
        ];
        for (text, want) in cases {
            let got = ConfigFile::parse(text).unwrap_err();
            assert_eq!(got.code(), want, "case={text:?}");
        }
    }

    #[test]
    fn a_failed_set_leaves_the_file_unchanged() {
        let mut file = ConfigFile::default();
        file.set(Key::RelayAddr, "none").unwrap();
        let before = file.clone();
        let err = file.set(Key::RelayAddr, "udp://bad_host:1").unwrap_err();
        assert_eq!(err, ConfigError::InvalidValue(Key::RelayAddr));
        assert_eq!(file, before);
        assert_eq!("nope".parse::<Key>(), Err(ConfigError::UnknownKey));
    }

    #[test]
    fn resetting_relay_id_also_removes_pinned_for() {
        let mut file = ConfigFile::default();
        file.set(Key::RelayId, ID).unwrap();
        file.set(Key::RelayPinnedFor, "relay.example.com:7400")
            .unwrap();
        file.set(Key::RelayAddr, "none").unwrap();
        file.reset(Key::RelayId);
        assert_eq!(file.get(Key::RelayPinnedFor), None);
        assert_eq!(file.get(Key::RelayAddr).as_deref(), Some("none"));
    }

    #[test]
    fn names_are_escaped_in_the_file() {
        let mut file = ConfigFile::default();
        file.set(Key::Name, "a \"quoted\" \\ name").unwrap();
        let back = ConfigFile::parse(&file.to_toml()).unwrap();
        assert_eq!(back, file);
        assert!(file.set(Key::Name, "  ").is_err(), "blank name");
        assert!(file.set(Key::Name, "a\nb").is_err(), "control character");
    }
}

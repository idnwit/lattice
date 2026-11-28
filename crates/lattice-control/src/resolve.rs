//! Precedence per key: flag > environment > file > default (specs/10-settings.md, rule 33).

use std::fmt;
use std::net::SocketAddr;

use serde::{Deserialize, Serialize};

use crate::config::{ConfigError, ConfigFile, Key};
use crate::relay::{RelayAddr, RelayPlan};

/// Default `listen.addr` (specs/10-settings.md, Worker config table).
pub const DEFAULT_LISTEN: SocketAddr =
    SocketAddr::new(std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED), 7441);

/// Environment variable of `relay.addr`.
pub const RELAY_ENV: &str = "LATTICE_RELAY";

/// Which layer a resolved value came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Source {
    /// A command-line flag of `latticed`.
    Flag,
    /// An environment variable.
    Env,
    /// `config.toml`.
    File,
    /// The schema default.
    Default,
}

impl Source {
    /// The text form: `flag`, `env`, `file`, `default`.
    fn as_str(self) -> &'static str {
        match self {
            Self::Flag => "flag",
            Self::Env => "env",
            Self::File => "file",
            Self::Default => "default",
        }
    }
}

impl fmt::Display for Source {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The layers above the file and the defaults that need the host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layers {
    flags: ConfigFile,
    env: ConfigFile,
    default_name: String,
}

impl Layers {
    /// No flags, no environment; `default_name` is the host name (Worker config table).
    pub fn new(default_name: String) -> Self {
        Self {
            flags: ConfigFile::default(),
            env: ConfigFile::default(),
            default_name,
        }
    }

    /// Adds a flag value.
    ///
    /// # Errors
    ///
    /// [`ConfigError::InvalidValue`] when `value` fails the key's rule.
    pub fn flag(mut self, key: Key, value: &str) -> Result<Self, ConfigError> {
        self.flags.set(key, value)?;
        Ok(self)
    }

    /// Adds an environment value.
    ///
    /// # Errors
    ///
    /// [`ConfigError::InvalidValue`] when `value` fails the key's rule.
    pub fn env(mut self, key: Key, value: &str) -> Result<Self, ConfigError> {
        self.env.set(key, value)?;
        Ok(self)
    }

    /// Resolves every key over `file`.
    pub fn resolve(&self, file: &ConfigFile) -> Resolved {
        let entries = Key::ALL
            .into_iter()
            .map(|key| self.entry(file, key))
            .collect();
        let layers = [&self.flags, &self.env, file];
        let relay = layers
            .into_iter()
            .find_map(ConfigFile::relay_addr)
            .cloned()
            .unwrap_or(RelayAddr::None);
        // `relay.id` and `relay.pinned_for` live only in the file (Worker config table).
        let relay = RelayPlan::new(&relay, file.pin().as_ref());
        let listen = layers
            .into_iter()
            .find_map(ConfigFile::listen_addr)
            .unwrap_or(DEFAULT_LISTEN);
        let name = layers
            .into_iter()
            .find_map(ConfigFile::name)
            .unwrap_or(&self.default_name)
            .to_owned();
        Resolved {
            entries,
            relay,
            listen,
            name,
        }
    }

    fn entry(&self, file: &ConfigFile, key: Key) -> Entry {
        let layers = [
            (&self.flags, Source::Flag),
            (&self.env, Source::Env),
            (file, Source::File),
        ];
        let found = layers
            .into_iter()
            .find_map(|(layer, source)| Some((layer.get(key)?, source)));
        let (value, source) = found.unwrap_or_else(|| (self.default(key), Source::Default));
        Entry::new(key.as_str().to_owned(), value, source)
    }

    /// The default of `key` in text form; empty for "no value".
    fn default(&self, key: Key) -> String {
        match key {
            Key::RelayAddr => RelayAddr::None.to_string(),
            Key::RelayId | Key::RelayPinnedFor => String::new(),
            Key::ListenAddr => DEFAULT_LISTEN.to_string(),
            Key::Name => self.default_name.clone(),
        }
    }
}

/// One resolved key: its value in text form and where it came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    key: String,
    value: String,
    source: Source,
}

impl Entry {
    /// An entry. `value` is empty for a key with no value.
    pub fn new(key: String, value: String, source: Source) -> Self {
        Self { key, value, source }
    }

    /// The dotted key name.
    pub fn key(&self) -> &str {
        &self.key
    }

    /// The value in text form.
    pub fn value(&self) -> &str {
        &self.value
    }

    /// The layer the value came from.
    pub fn source(&self) -> Source {
        self.source
    }
}

/// The resolved worker config.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    entries: Vec<Entry>,
    relay: RelayPlan,
    listen: SocketAddr,
    name: String,
}

impl Resolved {
    /// Every key, in [`Key::ALL`] order.
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// The entry of `key`.
    pub fn entry(&self, key: Key) -> Option<&Entry> {
        self.entries.iter().find(|e| e.key == key.as_str())
    }

    /// The home relay and the id to register with.
    pub fn relay(&self) -> &RelayPlan {
        &self.relay
    }

    /// The UDP listen address.
    pub fn listen(&self) -> SocketAddr {
        self.listen
    }

    /// The worker name.
    pub fn name(&self) -> &str {
        &self.name
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(r: &Resolved, key: Key) -> (String, Source) {
        let e = r.entry(key).unwrap();
        (e.value().to_owned(), e.source())
    }

    #[test]
    fn flag_beats_env_beats_file_beats_default() {
        let mut file = ConfigFile::default();
        file.set(Key::RelayAddr, "udp://file.example.com:1")
            .unwrap();
        let base = Layers::new("host".to_owned());
        let env = base
            .clone()
            .env(Key::RelayAddr, "udp://env.example.com:1")
            .unwrap();
        let flag = env.clone().flag(Key::RelayAddr, "none").unwrap();
        let cases = [
            (
                "default",
                &base,
                &ConfigFile::default(),
                "none",
                Source::Default,
            ),
            (
                "file",
                &base,
                &file,
                "udp://file.example.com:1",
                Source::File,
            ),
            ("env", &env, &file, "udp://env.example.com:1", Source::Env),
            ("flag", &flag, &file, "none", Source::Flag),
        ];
        for (name, layers, file, value, src) in cases {
            let r = layers.resolve(file);
            assert_eq!(
                source(&r, Key::RelayAddr),
                (value.to_owned(), src),
                "case={name}"
            );
        }
        assert_eq!(*flag.resolve(&file).relay(), RelayPlan::None);
    }

    #[test]
    fn defaults_fill_every_key() {
        let r = Layers::new("box".to_owned()).resolve(&ConfigFile::default());
        assert_eq!(r.entries().len(), Key::ALL.len());
        assert_eq!(
            source(&r, Key::ListenAddr),
            ("0.0.0.0:7441".to_owned(), Source::Default)
        );
        assert_eq!(source(&r, Key::Name), ("box".to_owned(), Source::Default));
        assert_eq!(source(&r, Key::RelayId), (String::new(), Source::Default));
        assert_eq!(r.listen(), DEFAULT_LISTEN);
        assert_eq!(r.name(), "box");
    }

    #[test]
    fn invalid_flag_or_env_values_fail() {
        let layers = Layers::new("h".to_owned());
        let err = layers
            .clone()
            .env(Key::RelayAddr, "udp://x_y:1")
            .unwrap_err();
        assert_eq!(err.code(), "config.invalid_value");
        assert!(layers.flag(Key::ListenAddr, "nope").is_err());
    }
}

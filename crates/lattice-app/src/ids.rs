//! Ids of apps, packages, instances, and processes (06-apps.md, Instances).

use serde::{Deserialize, Serialize};

/// Why an id was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum IdError {
    /// Empty id.
    #[error("empty id")]
    Empty,
    /// Too long or a byte outside the allowed set.
    #[error("invalid id")]
    Invalid,
}

impl IdError {
    /// Stable code for wire and logs.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Empty => "app.id_empty",
            Self::Invalid => "app.id_invalid",
        }
    }
}

/// Max length of an app id, in bytes (06-apps.md, Instances; proposed).
pub const MAX_APP_ID_LEN: usize = 128;

/// App id from `app.toml` (`dev.lattice.terminal`).
///
/// Valid ids (06-apps.md, Instances; proposed): reverse-DNS, at most
/// [`MAX_APP_ID_LEN`] bytes, two or more non-empty labels of `[a-z0-9-]`
/// separated by `.`. Validated again when deserialized.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct AppId(String);

impl AppId {
    /// Validates an app id.
    ///
    /// # Errors
    ///
    /// [`IdError`] when empty or not a reverse-DNS name.
    pub fn try_new(raw: &str) -> Result<Self, IdError> {
        if raw.is_empty() {
            return Err(IdError::Empty);
        }
        let label_ok = |label: &str| {
            !label.is_empty()
                && label
                    .bytes()
                    .all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'-'))
        };
        let labels_ok = raw.split('.').all(label_ok) && raw.contains('.');
        if raw.len() > MAX_APP_ID_LEN || !labels_ok {
            return Err(IdError::Invalid);
        }
        Ok(Self(raw.to_owned()))
    }

    /// The id text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for AppId {
    type Error = IdError;

    fn try_from(raw: String) -> Result<Self, IdError> {
        Self::try_new(&raw)
    }
}

impl From<AppId> for String {
    fn from(id: AppId) -> String {
        id.0
    }
}

/// Instance id (`iid`), assigned by the worker on `create`. Also the journal id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct InstanceId(u64);

impl InstanceId {
    /// Wraps a raw id.
    pub fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    /// The raw id.
    pub fn raw(self) -> u64 {
        self.0
    }
}

impl std::fmt::Display for InstanceId {
    /// Decimal id, as used in stream ids (`inst/<iid>/state`).
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// blake3 hash of a package (`app.toml`, `runner.wasm`, `viewer.wasm`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct PackageHash([u8; 32]);

impl PackageHash {
    /// Wraps a raw hash.
    pub fn from_bytes(raw: [u8; 32]) -> Self {
        Self(raw)
    }

    /// The raw hash.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl std::fmt::Display for PackageHash {
    /// Lowercase hex, as in cache file names.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.iter().try_for_each(|b| write!(f, "{b:02x}"))
    }
}

/// A process of the instance, as the runner names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ProcRef(u64);

impl ProcRef {
    /// Wraps a raw process id from `latticed-proc`.
    pub fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    /// The raw id.
    pub fn raw(self) -> u64 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_id_rejects_empty() {
        assert_eq!(AppId::try_new(""), Err(IdError::Empty));
    }

    #[test]
    fn app_id_is_reverse_dns() {
        for ok in ["dev.lattice.terminal", "test.echo", "a-1.b2"] {
            assert_eq!(
                AppId::try_new(ok).map(|a| a.as_str().to_owned()),
                Ok(ok.to_owned()),
                "case={ok}"
            );
        }
        for bad in ["terminal", ".a.b", "a.b.", "a..b", "A.b", "a.b c", "a_b.c"] {
            assert_eq!(AppId::try_new(bad), Err(IdError::Invalid), "case={bad}");
        }
    }

    #[test]
    fn app_id_max_length_is_128_bytes() {
        let max = format!("a.{}", "b".repeat(MAX_APP_ID_LEN - 2));
        assert!(AppId::try_new(&max).is_ok());
        let over = format!("a.{}", "b".repeat(MAX_APP_ID_LEN - 1));
        assert_eq!(AppId::try_new(&over), Err(IdError::Invalid));
    }

    #[test]
    fn app_id_is_validated_when_deserialized() {
        let bad = lattice_state::encode("Bad").unwrap();
        assert!(lattice_state::decode::<AppId>(&bad).is_err());
        let ok = lattice_state::encode(&AppId::try_new("a.b").unwrap()).unwrap();
        assert_eq!(lattice_state::decode::<AppId>(&ok).unwrap().as_str(), "a.b");
    }

    #[test]
    fn id_error_codes_are_stable_and_namespaced() {
        assert_eq!(IdError::Empty.code(), "app.id_empty");
        assert_eq!(IdError::Invalid.code(), "app.id_invalid");
    }

    #[test]
    fn raw_ids_roundtrip() {
        assert_eq!(InstanceId::from_raw(7).raw(), 7);
        assert_eq!(InstanceId::from_raw(7).to_string(), "7");
        assert_eq!(ProcRef::from_raw(3).raw(), 3);
        assert_eq!(
            PackageHash::from_bytes([1; 32]),
            PackageHash::from_bytes([1; 32])
        );
    }
}

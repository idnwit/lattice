//! [`Value`]: one value replaced as a whole.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{ApplyError, Revision, Synced};

/// A value set as a whole. `set` moves the old value into the draft; the
/// change is the new value. Use it for scalars and small records; a big collection is a
/// [`Map`](crate::Map) or [`List`](crate::List).
#[derive(Debug, Default)]
pub struct Value<T> {
    now: T,
    /// The committed value while a draft is open.
    old: Option<T>,
    rev: Revision,
}

impl<T> Value<T> {
    /// A committed value.
    pub fn new(value: T) -> Self {
        Self {
            now: value,
            old: None,
            rev: Revision::default(),
        }
    }

    /// The current value, draft included.
    pub fn get(&self) -> &T {
        &self.now
    }

    /// Replaces the value. The first `set` of a draft keeps the committed value; a later
    /// one drops the value in between. Setting the committed value again clears the draft: no
    /// change. O(1) plus one comparison.
    pub fn set(&mut self, value: T)
    where
        T: PartialEq,
    {
        if self.old.is_none() && self.now == value {
            return;
        }
        self.rev.bump();
        match self.old.take() {
            Some(old) if old == value => self.now = old,
            Some(old) => {
                self.now = value;
                self.old = Some(old);
            }
            None => self.old = Some(std::mem::replace(&mut self.now, value)),
        }
    }

    /// The committed value, without the draft.
    pub fn committed(&self) -> &T {
        self.old.as_ref().unwrap_or(&self.now)
    }
}

impl<T> From<T> for Value<T> {
    fn from(value: T) -> Self {
        Self::new(value)
    }
}

/// Compares the current values.
impl<T: PartialEq> PartialEq for Value<T> {
    fn eq(&self, other: &Self) -> bool {
        self.now == other.now
    }
}

impl<T: Serialize> Serialize for Value<T> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.now.serialize(s)
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Value<T> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        T::deserialize(d).map(Self::new)
    }
}

impl<T: Serialize + DeserializeOwned> Synced for Value<T> {
    type Change = T;

    fn has_draft(&self) -> bool {
        self.old.is_some()
    }

    fn write_change<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.now.serialize(s)
    }

    fn commit(&mut self) {
        self.old = None;
    }

    fn rollback(&mut self) {
        if let Some(old) = self.old.take() {
            self.now = old;
            self.rev.bump();
        }
    }

    fn apply(&mut self, change: T) -> Result<(), ApplyError> {
        if self.old.is_some() {
            return Err(ApplyError::Dirty);
        }
        self.now = change;
        self.rev.bump();
        Ok(())
    }

    fn rev(&self) -> u64 {
        self.rev.get()
    }
}

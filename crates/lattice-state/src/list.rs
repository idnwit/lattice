//! [`List`]: an ordered list of synced values that grows and shrinks at the end.
//!
//! Change space: `fresh` counts the items the draft pushed (they are the last ones); `cut` holds
//! the committed items the draft truncated off the end; `edited` holds the indexes of committed
//! items handed out with `get_mut`. Nothing ever shifts: there is no insert or remove before the
//! end. An app that needs one uses a [`Map`](crate::Map) with ordered keys.
//!
//! Invariant: `fresh <= items.len()`.

use std::collections::BTreeSet;

use serde::ser::{SerializeSeq, SerializeStruct};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{ApplyError, Draft, Revision, Synced};

/// An ordered list of synced values: `push`, `truncate` / `pop` / `clear` at the end, `get_mut`
/// in place.
#[derive(Debug)]
pub struct List<T> {
    items: Vec<T>,
    /// Number of items at the end pushed in the draft.
    fresh: usize,
    /// Committed items the draft truncated off the end, in order.
    cut: Vec<T>,
    /// Indexes of committed items handed out by `get_mut`.
    edited: BTreeSet<usize>,
    /// Bumped on every path to the items: the list's revision covers them.
    rev: Revision,
}

/// A committed change of a [`List`]: cut the list to `truncate` items, append `push`, then
/// change the items at `edits` (all below the cut). Every field is always encoded.
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListChange<T, C> {
    /// The length the list is cut to first, if it lost items.
    pub truncate: Option<usize>,
    /// Items appended, in order.
    pub push: Vec<T>,
    /// Changes inside items that stay: `(index, change)`, ascending.
    pub edits: Vec<(usize, C)>,
}

impl<T: Synced> List<T> {
    /// An empty list.
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            fresh: 0,
            cut: Vec::new(),
            edited: BTreeSet::new(),
            rev: Revision::default(),
        }
    }

    /// Number of committed items still in the list (the ones before the pushed ones).
    fn kept(&self) -> usize {
        // Invariant `fresh <= items.len()`: never saturates.
        self.items.len().saturating_sub(self.fresh)
    }

    /// The item at `index`, draft included.
    pub fn get(&self, index: usize) -> Option<&T> {
        self.items.get(index)
    }

    /// The item at `index`, to change through its own methods. Records the index.
    pub fn get_mut(&mut self, index: usize) -> Option<&mut T> {
        self.rev.bump();
        if index < self.kept() {
            self.edited.insert(index);
        }
        self.items.get_mut(index)
    }

    /// Appends `item`.
    pub fn push(&mut self, item: T) {
        self.items.push(item);
        // `fresh <= items.len()`, and a `Vec` holds at most `isize::MAX` items: never saturates.
        self.fresh = self.fresh.saturating_add(1);
        self.rev.bump();
    }

    /// Keeps the first `len` items. Committed items cut off move into the draft: nothing is
    /// cloned. O(items cut).
    pub fn truncate(&mut self, len: usize) {
        if len >= self.items.len() {
            return;
        }
        self.rev.bump();
        let kept = self.kept();
        self.items.truncate(len.max(kept));
        self.fresh = self.items.len().saturating_sub(kept);
        if len < kept {
            for i in self.edited.split_off(&len) {
                if let Some(item) = self.items.get_mut(i) {
                    item.rollback();
                }
            }
            // They sat before the items cut earlier in this draft.
            let later = std::mem::replace(&mut self.cut, self.items.split_off(len));
            self.cut.extend(later);
        }
    }

    /// Removes the last item; true when there was one.
    pub fn pop(&mut self) -> bool {
        let last = self.items.len().checked_sub(1);
        last.inspect(|last| self.truncate(*last)).is_some()
    }

    /// Removes every item.
    pub fn clear(&mut self) {
        self.truncate(0);
    }

    /// Items in order, draft included.
    pub fn iter(&self) -> std::slice::Iter<'_, T> {
        self.items.iter()
    }

    /// Number of items, draft included.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// True when the list has no item, draft included.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

impl<T: Synced> Default for List<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Committed items.
impl<T: Synced> FromIterator<T> for List<T> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        let mut items = Vec::from_iter(iter);
        items.iter_mut().for_each(T::commit);
        Self {
            items,
            ..Self::new()
        }
    }
}

impl<'a, T> IntoIterator for &'a List<T> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.items.iter()
    }
}

/// Compares the current items.
impl<T: PartialEq> PartialEq for List<T> {
    fn eq(&self, other: &Self) -> bool {
        self.items == other.items
    }
}

impl<T: Serialize> Serialize for List<T> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.items.serialize(s)
    }
}

impl<'de, T: Synced> Deserialize<'de> for List<T> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Vec::deserialize(d).map(|items| Self {
            items,
            ..Self::new()
        })
    }
}

impl<T: Synced> List<T> {
    /// The drafts of the committed items handed out, by index.
    fn edits(&self) -> impl Iterator<Item = (usize, Draft<'_, T>)> {
        self.edited.iter().filter_map(|i| {
            Some((
                *i,
                Draft(self.items.get(*i).filter(|item| item.has_draft())?),
            ))
        })
    }
}

/// The `edits` field of a draft, encoded without a buffer. postcard needs the length up front;
/// the filtered iterator does not know it, so count first (a second O(touched) walk).
struct Edits<'a, T>(&'a List<T>);

impl<T: Synced> Serialize for Edits<'_, T> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut out = s.serialize_seq(Some(self.0.edits().count()))?;
        for edit in self.0.edits() {
            out.serialize_element(&edit)?;
        }
        out.end()
    }
}

impl<T: Synced> Synced for List<T> {
    type Change = ListChange<T, T::Change>;

    fn has_draft(&self) -> bool {
        self.fresh > 0 || !self.cut.is_empty() || self.edits().next().is_some()
    }

    fn write_change<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let kept = self.kept();
        // Every field, in `ListChange` order: postcard cannot skip one.
        let mut out = s.serialize_struct("ListChange", 3)?;
        out.serialize_field("truncate", &(!self.cut.is_empty()).then_some(kept))?;
        out.serialize_field("push", self.items.get(kept..).unwrap_or_default())?;
        out.serialize_field("edits", &Edits(self))?;
        out.end()
    }

    fn commit(&mut self) {
        // A pushed item may carry a draft of its own (written before or after the push).
        let fresh = self.kept()..self.items.len();
        for i in std::mem::take(&mut self.edited).into_iter().chain(fresh) {
            if let Some(item) = self.items.get_mut(i) {
                item.commit();
            }
        }
        self.fresh = 0;
        self.cut.clear();
    }

    fn rollback(&mut self) {
        if self.fresh > 0 || !self.cut.is_empty() || !self.edited.is_empty() {
            self.rev.bump();
        }
        for i in std::mem::take(&mut self.edited) {
            if let Some(item) = self.items.get_mut(i) {
                item.rollback();
            }
        }
        self.items.truncate(self.kept());
        self.fresh = 0;
        self.items.append(&mut self.cut);
    }

    fn apply(&mut self, change: Self::Change) -> Result<(), ApplyError> {
        if self.fresh > 0 || !self.cut.is_empty() || !self.edited.is_empty() {
            return Err(ApplyError::Dirty);
        }
        self.rev.bump();
        if let Some(len) = change.truncate {
            if len > self.items.len() {
                return Err(ApplyError::OutOfRange);
            }
            self.items.truncate(len);
        }
        self.items.extend(change.push);
        for (at, change) in change.edits {
            self.items
                .get_mut(at)
                .ok_or(ApplyError::OutOfRange)?
                .apply(change)?;
        }
        Ok(())
    }

    fn rev(&self) -> u64 {
        self.rev.get()
    }
}

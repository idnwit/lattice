//! [`Map`]: an ordered map of synced values.
//!
//! Change space: `touched` holds each key written in the draft, once, with the committed value
//! an `insert` or `remove` moved out. Commit, rollback and the change walk `touched` only.

use std::borrow::Borrow;
use std::collections::BTreeMap;
use std::collections::btree_map::Entry;

use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{ApplyError, Draft, Key, Revision, Synced};

/// An ordered map of synced values. The change holds one [`MapOp`] per touched key, in key
/// order. `K` is a [`Key`].
#[derive(Debug)]
pub struct Map<K, T> {
    items: BTreeMap<K, T>,
    touched: BTreeMap<K, Touch<T>>,
    /// Bumped on every path to the items: the map's revision covers them.
    rev: Revision,
}

/// What the draft did to one key.
#[derive(Debug)]
enum Touch<T> {
    /// `get_mut` on a committed item: its own draft holds the change.
    Edited,
    /// `insert` or `remove`: the committed item moved here (`None`: the key was not there).
    Was(Option<T>),
}

/// The change of one key of a [`Map`]. `V` is the item (owned in a change, borrowed in a
/// draft), `C` the item's change. Encodes as an enum variant index, then the payload.
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MapOp<V, C> {
    /// The key now holds this item (new or replaced).
    Set(V),
    /// The key is gone.
    Remove,
    /// The item at the key changed inside.
    Edit(C),
}

impl<K, T> Map<K, T> {
    /// An empty map.
    pub fn new() -> Self {
        Self::from_items(BTreeMap::new())
    }

    /// A map of committed items.
    fn from_items(items: BTreeMap<K, T>) -> Self {
        Self {
            items,
            touched: BTreeMap::new(),
            rev: Revision::default(),
        }
    }
}

impl<K, T> Default for Map<K, T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: Ord + Clone, T: Synced> Map<K, T> {
    /// The item at `key`, draft included.
    pub fn get<Q: Ord + ?Sized>(&self, key: &Q) -> Option<&T>
    where
        K: Borrow<Q>,
    {
        self.items.get(key)
    }

    /// True when `key` is in the map, draft included.
    pub fn contains_key<Q: Ord + ?Sized>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
    {
        self.items.contains_key(key)
    }

    /// The item at `key`, to change through its own methods. Records the key (one key clone,
    /// the first time). O(log n).
    pub fn get_mut(&mut self, key: &K) -> Option<&mut T> {
        let item = self.items.get_mut(key)?;
        self.rev.bump();
        if !self.touched.contains_key(key) {
            self.touched.insert(key.clone(), Touch::Edited);
        }
        Some(item)
    }

    /// The item at `key`, to change through its own methods; an absent key gets the default
    /// item first, as an `insert` would. O(log n).
    pub fn entry(&mut self, key: K) -> &mut T
    where
        T: Default,
    {
        self.rev.bump();
        if !self.touched.contains_key(&key) {
            let touch = if self.items.contains_key(&key) {
                Touch::Edited
            } else {
                Touch::Was(None)
            };
            self.touched.insert(key.clone(), touch);
        }
        self.items.entry(key).or_default()
    }

    /// Puts `item` at `key`. A committed item there moves into the draft. O(log n).
    pub fn insert(&mut self, key: K, item: T) {
        self.rev.bump();
        match self.touched.get_mut(&key) {
            // The item replaced was made in this draft: nothing to keep.
            Some(Touch::Was(_)) => {
                self.items.insert(key, item);
            }
            Some(touch) => {
                let mut was = self.items.insert(key, item);
                if let Some(old) = &mut was {
                    old.rollback();
                }
                *touch = Touch::Was(was);
            }
            None => {
                let was = self.items.insert(key.clone(), item);
                self.touched.insert(key, Touch::Was(was));
            }
        }
    }

    /// Removes `key`; true when it was there. A committed item moves into the draft. O(log n).
    pub fn remove(&mut self, key: &K) -> bool {
        let Some((key, item)) = self.items.remove_entry(key) else {
            return false;
        };
        self.rev.bump();
        self.took(key, item);
        true
    }

    /// Removes every item. Committed items move into the draft with their keys: nothing is
    /// cloned. O(n).
    pub fn clear(&mut self) {
        if !self.items.is_empty() {
            self.rev.bump();
        }
        for (key, item) in std::mem::take(&mut self.items) {
            self.took(key, item);
        }
    }

    /// Records that `item` left `key` in this draft.
    fn took(&mut self, key: K, mut item: T) {
        match self.touched.entry(key) {
            Entry::Vacant(touch) => {
                touch.insert(Touch::Was(Some(item)));
            }
            Entry::Occupied(mut touch) => match touch.get_mut() {
                // Made and removed in one draft: no change.
                Touch::Was(None) => {
                    touch.remove();
                }
                Touch::Was(Some(_)) => {}
                touch @ Touch::Edited => {
                    item.rollback();
                    *touch = Touch::Was(Some(item));
                }
            },
        }
    }

    /// Items in key order, draft included.
    pub fn iter(&self) -> impl DoubleEndedIterator<Item = (&K, &T)> + ExactSizeIterator {
        self.items.iter()
    }

    /// Keys in order, draft included.
    pub fn keys(&self) -> impl DoubleEndedIterator<Item = &K> + ExactSizeIterator {
        self.items.keys()
    }

    /// Number of items, draft included.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// True when the map has no item, draft included.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// The committed item at `key`, without this map's draft. The item has its own committed
    /// read for a change inside it.
    pub fn committed_get(&self, key: &K) -> Option<&T> {
        match self.touched.get(key) {
            Some(Touch::Was(was)) => was.as_ref(),
            _ => self.items.get(key),
        }
    }
}

/// Committed items.
impl<K: Ord + Clone, T: Synced> FromIterator<(K, T)> for Map<K, T> {
    fn from_iter<I: IntoIterator<Item = (K, T)>>(iter: I) -> Self {
        let mut items = BTreeMap::from_iter(iter);
        items.values_mut().for_each(T::commit);
        Self::from_items(items)
    }
}

/// Compares the current items.
impl<K: PartialEq, T: PartialEq> PartialEq for Map<K, T> {
    fn eq(&self, other: &Self) -> bool {
        self.items == other.items
    }
}

impl<K: Serialize, T: Serialize> Serialize for Map<K, T> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.items.serialize(s)
    }
}

impl<'de, K: Ord + Deserialize<'de>, T: Deserialize<'de>> Deserialize<'de> for Map<K, T> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        BTreeMap::deserialize(d).map(Self::from_items)
    }
}

impl<K: Ord, T: Synced> Map<K, T> {
    /// The draft, one op per key it really changes; set items and item drafts are read in place.
    fn ops(&self) -> impl Iterator<Item = (&K, MapOp<&T, Draft<'_, T>>)> {
        self.touched.iter().filter_map(|(key, touch)| {
            let op = match (touch, self.items.get(key)) {
                (Touch::Edited, item) => MapOp::Edit(Draft(item.filter(|item| item.has_draft())?)),
                (Touch::Was(_), Some(item)) => MapOp::Set(item),
                (Touch::Was(_), None) => MapOp::Remove,
            };
            Some((key, op))
        })
    }
}

impl<K: Key, T: Synced> Synced for Map<K, T> {
    type Change = BTreeMap<K, MapOp<T, T::Change>>;

    fn has_draft(&self) -> bool {
        self.ops().next().is_some()
    }

    fn write_change<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        // postcard needs the length up front; a filtered iterator does not know it, so count
        // first (a second O(touched) walk, no allocation).
        let mut out = s.serialize_map(Some(self.ops().count()))?;
        for (key, op) in self.ops() {
            out.serialize_entry(key, &op)?;
        }
        out.end()
    }

    fn commit(&mut self) {
        // A set item may carry a draft of its own (written before or after the insert).
        for key in std::mem::take(&mut self.touched).into_keys() {
            if let Some(item) = self.items.get_mut(&key) {
                item.commit();
            }
        }
    }

    fn rollback(&mut self) {
        if !self.touched.is_empty() {
            self.rev.bump();
        }
        for (key, touch) in std::mem::take(&mut self.touched) {
            match touch {
                Touch::Edited => {
                    if let Some(item) = self.items.get_mut(&key) {
                        item.rollback();
                    }
                }
                Touch::Was(Some(old)) => {
                    self.items.insert(key, old);
                }
                Touch::Was(None) => {
                    self.items.remove(&key);
                }
            }
        }
    }

    fn apply(&mut self, change: Self::Change) -> Result<(), ApplyError> {
        if !self.touched.is_empty() {
            return Err(ApplyError::Dirty);
        }
        self.rev.bump();
        for (key, op) in change {
            match op {
                MapOp::Set(item) => {
                    self.items.insert(key, item);
                }
                MapOp::Remove => {
                    self.items.remove(&key).ok_or(ApplyError::Missing)?;
                }
                MapOp::Edit(change) => self
                    .items
                    .get_mut(&key)
                    .ok_or(ApplyError::Missing)?
                    .apply(change)?,
            }
        }
        Ok(())
    }

    fn rev(&self) -> u64 {
        self.rev.get()
    }
}

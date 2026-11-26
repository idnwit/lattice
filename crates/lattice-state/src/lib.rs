//! Synced data types for app state: values that record their own draft.
//!
//! An app's state is a struct of synced types (`docs/architecture/06-apps.md` › State). Each
//! type records its own draft, so one reducer call costs O(touched), never O(state). The runner
//! commits a draft and encodes it as a change; a deck applies the decoded change on its replica.
//! This crate owns the types and their laws. It does no I/O.
//!
//! Ported from realtime-workspace rtf-core (RFC-0005 §6).
//!
//! # Contract ([`Synced`])
//!
//! - A write goes through a method of the type. The old value is *moved* into the draft
//!   (`std::mem::replace`), never cloned. A container records each touched key once in its
//!   change space; the child it hands out with `get_mut` tracks itself.
//! - [`Synced::write_change`] encodes the draft as a change, by reference: the encoder reads the
//!   new values where they live ([`Draft`] is the draft as a serde value). Then
//!   [`Synced::commit`] drops the old values. [`Synced::rollback`] moves them back. All
//!   O(touched).
//! - [`Synced::apply`] puts a decoded [`Synced::Change`] on a replica. `write_change` and
//!   `Change` have one encoding: postcard (decision D19), canonical by struct field order and
//!   `BTreeMap` key order. Every field is always written: postcard cannot skip one.
//! - `Serialize` / `Deserialize` of a type is its snapshot: the current value, no draft.
//! - [`Value`] and [`Map`] read their committed value while a draft is open (`committed…`).
//! - No type here is `Clone`. One reducer call runs at a time, so nothing is shared.
//! - [`Synced::rev`] is the node's revision, for a UI only: it grows whenever what the node shows
//!   may have changed (a write, a `get_mut`, a rollback of a draft, an apply). A leaf holds a
//!   [`Revision`]; a container bumps its own on every path to its items, so its revision covers
//!   them; a derived struct sums its fields. Never encoded.
//!
//! # Laws
//!
//! Tested in `tests/synced.rs` and `tests/props.rs`: `apply` of the written change on a replica
//! gives the same value; `rollback` gives the last committed value; decode then encode of a
//! change gives the same bytes; `apply` on a value with a draft is refused
//! ([`ApplyError::Dirty`]); a write, a rollback of a draft and an apply raise `rev`, a read does
//! not.
//!
//! # Usage
//!
//! ```
//! use lattice_state::{Draft, List, Map, Synced, Value, decode, encode};
//! use serde::{Deserialize, Serialize};
//!
//! #[derive(Debug, Default, PartialEq, Serialize, Deserialize, Synced)]
//! struct Board {
//!     title: Value<String>,
//!     cards: List<Value<String>>,
//!     votes: Map<String, Value<u32>>,
//! }
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut board = Board::default();
//! // A replica starts from a snapshot.
//! let mut replica: Board = decode(&encode(&board)?)?;
//!
//! // One reducer call: write through the types' methods.
//! board.title.set("Sprint".to_owned());
//! board.cards.push(Value::new("ship it".to_owned()));
//! board.votes.insert("ana".to_owned(), Value::new(1));
//!
//! // Accepted: encode the draft as a change, then commit.
//! let change = encode(&Draft(&board))?;
//! board.commit();
//!
//! // The replica follows by the change alone.
//! replica.apply(decode(&change)?)?;
//! assert_eq!(replica, board);
//! # Ok(())
//! # }
//! ```
//!
//! # Key items
//!
//! - [`Synced`]: the trait, and `#[derive(Synced)]` for structs (same name, as serde does).
//! - [`Value`], [`Map`] (with [`MapOp`]), [`List`] (with [`ListChange`]): the synced types.
//! - [`Draft`]: a value's draft as a serde value; encodes as its [`Synced::Change`].
//! - [`encode`], [`decode`]: postcard bytes of a snapshot or a change.
//! - [`Revision`]: a node's local revision, for UI change detection.
//! - [`Key`]: what a [`Map`] can be keyed by.
//! - [`ApplyError`], [`EncodeError`]: failures, with a stable `code()`.

#![deny(clippy::indexing_slicing, clippy::arithmetic_side_effects)]
#![deny(clippy::cast_possible_truncation)]

mod list;
mod map;
mod value;

use std::cell::Cell;

use serde::de::DeserializeOwned;
use serde::{Serialize, Serializer};

pub use lattice_state_macros::Synced;
pub use list::{List, ListChange};
pub use map::{Map, MapOp};
pub use value::Value;

/// Paths the derive's generated code names. Not public API.
#[doc(hidden)]
pub mod __private {
    pub use serde;
}

/// A value inside an app state that records its own draft.
///
/// Runner: run the reducer; on `Ok`, encode a [`Draft`] of the state, then
/// [`commit`](Self::commit); on `Err`, [`rollback`](Self::rollback). Deck: [`apply`](Self::apply)
/// the decoded change.
pub trait Synced: Serialize + DeserializeOwned {
    /// A committed change, owned: what a replica decodes and applies.
    type Change: Serialize + DeserializeOwned;

    /// True when the draft changes the value. O(touched).
    fn has_draft(&self) -> bool;
    /// Writes the draft as a change, reading the new values in place: no copy.
    /// Encodes exactly as [`Self::Change`]. Call it when [`has_draft`](Self::has_draft).
    ///
    /// # Errors
    /// The serializer's error.
    fn write_change<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error>;
    /// Makes the draft the committed value: drops the old values. O(touched).
    fn commit(&mut self);
    /// Drops the draft, moving the old values back. O(touched).
    fn rollback(&mut self);
    /// Applies a committed change on a replica.
    ///
    /// # Errors
    /// [`ApplyError::Dirty`] when the changed part has a draft; [`ApplyError::Missing`] or
    /// [`ApplyError::OutOfRange`] when the change does not fit the replica. On any error the
    /// value is unspecified: reload it from a snapshot.
    fn apply(&mut self, change: Self::Change) -> Result<(), ApplyError>;
    /// The revision of the value: it grows whenever the value may have changed (a write, a
    /// `get_mut`, a rollback of a draft, an apply; not a commit) and never goes back. A new
    /// value has a revision above every one given out before on this thread, so a replaced
    /// node reads as changed. Local to this replica, never encoded: a UI watches it instead of
    /// copying data.
    fn rev(&self) -> u64;
}

thread_local! {
    /// The highest revision given out on this thread.
    static REVISIONS: Cell<u64> = const { Cell::new(0) };
}

/// The revision of one synced node: what [`Synced::rev`] of a leaf or a container returns.
///
/// A new one is above every revision given out before on this thread; [`Revision::bump`]
/// raises it above that too, and above itself, so it never goes back even when the node moves
/// to another thread.
#[derive(Debug)]
pub struct Revision(u64);

impl Revision {
    /// The revision now.
    pub fn get(&self) -> u64 {
        self.0
    }

    /// Marks a change.
    pub fn bump(&mut self) {
        self.0 = next(self.0);
    }
}

impl Default for Revision {
    fn default() -> Self {
        Self(next(0))
    }
}

/// The next revision on this thread, above `at_least`. Saturates at `u64::MAX`, which one
/// thread cannot reach by counting.
fn next(at_least: u64) -> u64 {
    REVISIONS.with(|top| {
        let rev = top.get().max(at_least).saturating_add(1);
        top.set(rev);
        rev
    })
}

/// The draft of a synced value as a serde value: encodes as its [`Synced::Change`], through
/// [`Synced::write_change`].
#[derive(Debug)]
pub struct Draft<'a, T>(pub &'a T);

impl<T: Synced> Serialize for Draft<'_, T> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.0.write_change(s)
    }
}

/// A key of a [`Map`]: totally ordered, so a change lists its keys in one canonical order, and
/// encoded by postcard without fail and the same way every time. Implemented for strings,
/// `char` and the integers; an app may implement it for its own id newtypes
/// (`#[serde(transparent)]` over one of those).
pub trait Key: Ord + Clone + Serialize + DeserializeOwned {}

/// Implements [`Key`] for std types that encode as a postcard string, char or varint.
macro_rules! keys {
    ($($t:ty),*) => {$( impl Key for $t {} )*};
}
keys!(
    String, char, u8, u16, u32, u64, u128, usize, i8, i16, i32, i64, i128, isize
);

/// Encodes a snapshot or a change (wrap a draft in [`Draft`]) as postcard bytes.
///
/// # Errors
/// [`EncodeError`] when `value`'s `Serialize` asks for something postcard cannot write, such
/// as a sequence or map of unknown length. The types of this crate never do; an app type
/// inside a [`Value`] might.
pub fn encode<T: Serialize + ?Sized>(value: &T) -> Result<Vec<u8>, EncodeError> {
    postcard::to_allocvec(value).map_err(|_| EncodeError)
}

/// Decodes a snapshot or a [`Synced::Change`] from postcard bytes.
///
/// # Errors
/// [`ApplyError::Decode`] when the bytes are not one whole `T`.
pub fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, ApplyError> {
    match postcard::take_from_bytes(bytes) {
        Ok((value, [])) => Ok(value),
        _ => Err(ApplyError::Decode),
    }
}

/// Decodes a value that borrows from `bytes` (e.g. a `&[u8]` field), the whole buffer.
///
/// # Errors
/// [`ApplyError::Decode`] when the bytes are not exactly one `T`.
pub fn decode_borrowed<'a, T: serde::Deserialize<'a>>(bytes: &'a [u8]) -> Result<T, ApplyError> {
    match postcard::take_from_bytes(bytes) {
        Ok((value, [])) => Ok(value),
        _ => Err(ApplyError::Decode),
    }
}

/// Why [`encode`] failed: the value's `Serialize` does not fit postcard (a bug in that type).
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("value does not encode as postcard")]
pub struct EncodeError;

impl EncodeError {
    /// Stable code for logs.
    pub fn code(&self) -> &'static str {
        "state_encode"
    }
}

/// Why [`Synced::apply`] or [`decode`] refused a change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ApplyError {
    /// The value has a draft: roll it back or commit it first.
    #[error("draft not empty")]
    Dirty,
    /// The change names a key the replica does not have.
    #[error("no such item")]
    Missing,
    /// The change names a position outside the value.
    #[error("position out of range")]
    OutOfRange,
    /// The bytes do not decode as the type's value or change.
    #[error("change does not decode")]
    Decode,
}

impl ApplyError {
    /// Stable code for logs and the wire.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Dirty => "state_dirty",
            Self::Missing => "state_missing",
            Self::OutOfRange => "state_out_of_range",
            Self::Decode => "state_decode",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_refuses_trailing_and_short_bytes() {
        let cases: [(&str, &[u8]); 3] = [
            ("trailing", &[1, 2]),
            ("short", &[]),
            ("bad varint", &[0xFF]),
        ];
        for (name, bytes) in cases {
            assert_eq!(decode::<u32>(bytes), Err(ApplyError::Decode), "case={name}");
        }
        assert_eq!(decode::<u32>(&[1]), Ok(1));
    }

    #[test]
    fn revisions_grow_and_new_ones_are_above_old() {
        let mut a = Revision::default();
        let b = Revision::default();
        assert!(b.get() > a.get());
        let before = a.get();
        a.bump();
        assert!(a.get() > before && a.get() > b.get());
    }

    #[test]
    fn error_codes_are_distinct() {
        let codes = [
            ApplyError::Dirty.code(),
            ApplyError::Missing.code(),
            ApplyError::OutOfRange.code(),
            ApplyError::Decode.code(),
            EncodeError.code(),
        ];
        let unique: std::collections::BTreeSet<_> = codes.iter().collect();
        assert_eq!(unique.len(), codes.len());
    }
}

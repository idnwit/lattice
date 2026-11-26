//! Laws of the synced data types (`docs/architecture/06-apps.md` › State), checked on random
//! but deterministic op sequences (a small seeded xorshift generator). `tests/props.rs` checks
//! `List` and `Map` against independent models with proptest.
//!
//! For each type, many rounds of: random writes, then either
//! - rollback: the value equals the last committed one (law b); or
//! - commit: the encoded draft, decoded and applied on a replica built from the first snapshot,
//!   gives the same value (law a); decode then encode gives the same bytes (law c); `apply` on
//!   the value while its draft is open is refused (law d).
//!
//! Before either, the committed read equals the replica.
//!
//! `rev`: a write that changes the value, a rollback of a draft and an apply raise it; a read
//! leaves it; nothing lowers it.
//!
//! Every encoding is postcard, through `lattice_state::{encode, decode}`.

use std::collections::BTreeMap;
use std::fmt::Debug;

use lattice_state::{
    ApplyError, Draft, Key, List, ListChange, Map, MapOp, Synced, Value, decode, encode,
};
use serde::{Deserialize, Serialize};

type Res = Result<(), Box<dyn std::error::Error>>;

/// xorshift64: deterministic, seeded per test.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        let n = u64::try_from(n.max(1)).unwrap_or(u64::MAX);
        usize::try_from(self.next() % n).unwrap_or(0)
    }
}

/// A random plain value, from a small space so keys and values collide.
trait Arb {
    fn arb(rng: &mut Rng) -> Self;
}

impl Arb for i32 {
    fn arb(rng: &mut Rng) -> Self {
        i32::try_from(rng.below(5)).unwrap_or(0)
    }
}

impl Arb for u16 {
    fn arb(rng: &mut Rng) -> Self {
        u16::try_from(rng.below(6)).unwrap_or(0)
    }
}

impl Arb for bool {
    fn arb(rng: &mut Rng) -> Self {
        rng.below(2) == 0
    }
}

impl Arb for (i32, i32) {
    fn arb(rng: &mut Rng) -> Self {
        (i32::arb(rng), i32::arb(rng))
    }
}

impl Arb for String {
    fn arb(rng: &mut Rng) -> Self {
        let words = ["a", "bé", "c→d", "", "k4", "k5"];
        words
            .get(rng.below(words.len()))
            .copied()
            .unwrap_or("")
            .into()
    }
}

/// A synced type the tests can write at random and compare with its committed replica.
trait Fuzz: Synced + Default + PartialEq + Debug {
    /// Some random writes (maybe none).
    fn fuzz(&mut self, rng: &mut Rng);
    /// Asserts that the committed read of `self` is `committed`.
    fn check_committed(&self, committed: &Self);
    /// A new value, sometimes carrying a draft of its own.
    fn make(rng: &mut Rng) -> Self {
        let mut v = Self::default();
        v.fuzz(rng);
        if rng.below(2) == 0 {
            v.commit();
        }
        v
    }
}

impl<T: Arb + Default + PartialEq + Debug + Serialize + for<'de> Deserialize<'de>> Fuzz
    for Value<T>
{
    fn fuzz(&mut self, rng: &mut Rng) {
        for _ in 0..rng.below(3) {
            self.set(T::arb(rng));
        }
    }
    fn check_committed(&self, committed: &Self) {
        assert_eq!(self.committed(), committed.get());
    }
}

impl<K: Arb + Key + Debug, T: Fuzz> Fuzz for Map<K, T> {
    fn fuzz(&mut self, rng: &mut Rng) {
        for _ in 0..rng.below(5) {
            let key = K::arb(rng);
            match rng.below(4) {
                0 => self.insert(key, T::make(rng)),
                1 => {
                    let there = self.contains_key(&key);
                    assert_eq!(self.remove(&key), there);
                    assert!(!self.contains_key(&key));
                }
                _ => {
                    if let Some(item) = self.get_mut(&key) {
                        item.fuzz(rng);
                    }
                }
            }
        }
    }
    fn check_committed(&self, committed: &Self) {
        for (key, item) in committed.iter() {
            let mine = self.committed_get(key);
            assert!(mine.is_some(), "committed key {key:?} lost");
            if let Some(mine) = mine {
                mine.check_committed(item);
            }
        }
        for (key, _) in self.iter() {
            assert_eq!(
                self.committed_get(key).is_some(),
                committed.contains_key(key),
                "{key:?}"
            );
        }
    }
}

impl<T: Fuzz> Fuzz for List<T> {
    fn fuzz(&mut self, rng: &mut Rng) {
        for _ in 0..rng.below(5) {
            let at = rng.below(self.len().saturating_add(1));
            match rng.below(7) {
                0 | 1 => self.push(T::make(rng)),
                2 => self.truncate(at),
                3 => {
                    let some = !self.is_empty();
                    assert_eq!(self.pop(), some);
                }
                _ => {
                    if let Some(item) = self.get_mut(at) {
                        item.fuzz(rng);
                    }
                }
            }
        }
    }
    /// A list has no committed read of its own.
    fn check_committed(&self, _: &Self) {}
}

/// A derived struct with one field of each type, like a whiteboard object.
#[derive(Debug, Default, PartialEq, Serialize, Deserialize, lattice_state::Synced)]
struct Shape {
    x: Value<i32>,
    label: Value<String>,
    points: List<Value<(i32, i32)>>,
    tags: Map<String, Value<bool>>,
}

impl Fuzz for Shape {
    fn fuzz(&mut self, rng: &mut Rng) {
        for _ in 0..rng.below(4) {
            match rng.below(4) {
                0 => self.x.fuzz(rng),
                1 => self.label.fuzz(rng),
                2 => self.points.fuzz(rng),
                _ => self.tags.fuzz(rng),
            }
        }
    }
    fn check_committed(&self, committed: &Self) {
        self.x.check_committed(&committed.x);
        self.label.check_committed(&committed.label);
        self.points.check_committed(&committed.points);
        self.tags.check_committed(&committed.tags);
    }
}

/// Law c (decode then encode gives the same bytes), law d (a value with a draft refuses a
/// change), then commit and law a (the replica follows by the change alone).
fn commit_and_follow<T: Fuzz>(value: &mut T, replica: &mut T, seed: u64) -> Res {
    let written = value.rev();
    let bytes = encode(&Draft(&*value))?;
    let change: T::Change = decode(&bytes)?;
    assert_eq!(encode(&change)?, bytes, "seed {seed}: {bytes:?}");
    assert_eq!(
        value.apply(decode(&bytes)?),
        Err(ApplyError::Dirty),
        "seed {seed}: {bytes:?}"
    );
    value.commit();
    assert!(!value.has_draft(), "seed {seed}");
    assert!(
        value.rev() >= written,
        "seed {seed}: commit never lowers rev"
    );
    let applied = replica.rev();
    assert_eq!(replica.apply(change), Ok(()), "seed {seed}: {bytes:?}");
    assert!(replica.rev() > applied, "seed {seed}: apply raises rev");
    Ok(())
}

/// Runs the laws on `T` for `seeds` op sequences of `rounds` drafts each.
fn laws<T: Fuzz>(seeds: u64, rounds: usize) -> Res {
    for seed in 1..=seeds {
        let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15));
        let mut value = T::make(&mut rng);
        value.commit();
        // The replica starts from a snapshot and only ever sees encoded changes.
        let mut replica: T = decode(&encode(&value)?)?;
        assert_eq!(replica, value);
        for _ in 0..rounds {
            let before = value.rev();
            value.fuzz(&mut rng);
            assert!(value.rev() >= before, "seed {seed}: rev never goes back");
            if value != replica {
                assert!(value.rev() > before, "seed {seed}: a write raises rev");
            }
            let written = value.rev();
            value.check_committed(&replica);
            encode(&value)?;
            assert_eq!(value.rev(), written, "seed {seed}: a read leaves rev");
            if rng.below(3) == 0 {
                // Law b: rollback gives the last committed value, and no draft.
                let drafted = value.has_draft();
                value.rollback();
                assert!(!value.has_draft(), "seed {seed}");
                assert!(
                    value.rev() >= written && (!drafted || value.rev() > written),
                    "seed {seed}: rollback raises rev"
                );
            } else if value.has_draft() {
                commit_and_follow(&mut value, &mut replica, seed)?;
            } else {
                // Touched, nothing changed: commit only clears the change space.
                value.commit();
            }
            assert_eq!(value, replica, "seed {seed}");
            assert_eq!(encode(&value)?, encode(&replica)?, "seed {seed}");
            value.check_committed(&replica);
        }
    }
    Ok(())
}

#[test]
fn value_laws() -> Res {
    laws::<Value<i32>>(200, 20)?;
    laws::<Value<String>>(200, 20)
}

#[test]
fn map_laws() -> Res {
    laws::<Map<String, Value<i32>>>(1000, 30)?;
    laws::<Map<u16, Map<String, Value<i32>>>>(1000, 30)
}

#[test]
fn list_laws() -> Res {
    laws::<List<Value<i32>>>(2000, 30)?;
    laws::<List<List<Value<i32>>>>(1000, 30)
}

#[test]
fn derive_laws() -> Res {
    laws::<Shape>(1000, 30)?;
    laws::<Map<u16, Shape>>(1000, 30)?;
    laws::<List<Shape>>(500, 30)
}

/// Encodes `value`'s draft, decodes it as `T::Change`, checks it re-encodes to the same bytes,
/// commits, applies it on `replica`, and checks both sides agree.
fn roundtrip<T: Synced + PartialEq + Debug>(value: &mut T, replica: &mut T) -> Res {
    assert!(value.has_draft());
    let bytes = encode(&Draft(&*value))?;
    let change: T::Change = decode(&bytes)?;
    assert_eq!(encode(&change)?, bytes);
    value.commit();
    replica.apply(change)?;
    assert_eq!(replica, value);
    assert_eq!(encode(&*replica)?, encode(&*value)?);
    Ok(())
}

/// Every type's change, including the empty fields postcard cannot skip, roundtrips through
/// postcard encode/decode and `apply`.
#[test]
fn every_change_roundtrips_through_postcard() -> Res {
    let mut v = Value::new(1);
    let mut rv = Value::new(1);
    v.set(-7);
    roundtrip(&mut v, &mut rv)?;

    let mut m: Map<String, Value<i32>> = Map::new();
    let mut rm: Map<String, Value<i32>> = Map::new();
    m.insert("a".into(), Value::new(1));
    m.insert("b".into(), Value::new(2));
    roundtrip(&mut m, &mut rm)?;
    m.remove(&"a".to_owned());
    if let Some(b) = m.get_mut(&"b".to_owned()) {
        b.set(20);
    }
    roundtrip(&mut m, &mut rm)?;

    let mut l: List<Value<i32>> = List::new();
    let mut rl: List<Value<i32>> = List::new();
    l.push(Value::new(1));
    l.push(Value::new(2));
    roundtrip(&mut l, &mut rl)?; // push only
    l.pop();
    roundtrip(&mut l, &mut rl)?; // truncate only
    if let Some(item) = l.get_mut(0) {
        item.set(9);
    }
    roundtrip(&mut l, &mut rl)?; // edits only

    let mut s = Shape::default();
    let mut rs = Shape::default();
    s.label.set("only label".into());
    roundtrip(&mut s, &mut rs)?; // one field of four
    s.x.set(3);
    s.points.push(Value::new((1, 2)));
    s.tags.insert("on".into(), Value::new(true));
    roundtrip(&mut s, &mut rs)?;

    let mut nested: Map<u16, List<Shape>> = Map::new();
    let mut rnested: Map<u16, List<Shape>> = Map::new();
    nested.entry(4).push(Shape::default());
    roundtrip(&mut nested, &mut rnested)?;
    if let Some(shape) = nested.get_mut(&4).and_then(|l| l.get_mut(0)) {
        shape.tags.insert("x".into(), Value::new(false));
    }
    roundtrip(&mut nested, &mut rnested)
}

/// `set` keeps the committed value once; the change is the new value.
#[test]
fn value_draft() -> Res {
    let mut v = Value::new(1);
    assert!(!v.has_draft());
    v.set(2);
    v.set(3);
    assert_eq!((*v.get(), *v.committed()), (3, 1));
    assert_eq!(decode::<i32>(&encode(&Draft(&v))?)?, 3);
    v.rollback();
    assert_eq!((*v.get(), *v.committed()), (1, 1));
    Ok(())
}

/// One op per touched key, in key order; an untouched or unchanged key is not in the change.
#[test]
fn map_change_per_key() -> Res {
    let mut m: Map<String, Value<i32>> = [("a", 1), ("b", 2), ("c", 3), ("d", 4)]
        .into_iter()
        .map(|(k, v)| (k.to_owned(), Value::new(v)))
        .collect();
    m.insert("e".into(), Value::new(5));
    m.remove(&"c".to_owned());
    if let Some(b) = m.get_mut(&"b".to_owned()) {
        b.set(20);
    }
    assert!(
        m.get_mut(&"a".to_owned()).is_some(),
        "handed out, not changed"
    );
    m.insert("z".into(), Value::new(0));
    m.remove(&"z".to_owned());
    let change: BTreeMap<String, MapOp<Value<i32>, i32>> = decode(&encode(&Draft(&m))?)?;
    let want = BTreeMap::from([
        ("b".to_owned(), MapOp::Edit(20)),
        ("c".to_owned(), MapOp::Remove),
        ("e".to_owned(), MapOp::Set(Value::new(5))),
    ]);
    assert_eq!(change, want);
    assert_eq!(m.committed_get(&"c".to_owned()).map(Value::get), Some(&3));
    assert!(m.committed_get(&"e".to_owned()).is_none());

    let mut replica: Map<String, Value<i32>> = Map::new();
    let missing = [("q".to_owned(), MapOp::Edit(1))].into_iter().collect();
    assert_eq!(replica.apply(missing), Err(ApplyError::Missing));
    Ok(())
}

/// `clear` removes every committed item and forgets items made in the draft; rollback brings
/// the committed ones back.
#[test]
fn map_clear() -> Res {
    let mut m: Map<String, Value<i32>> = [("a", 1), ("b", 2)]
        .into_iter()
        .map(|(k, v)| (k.to_owned(), Value::new(v)))
        .collect();
    let snapshot = encode(&m)?;
    m.insert("c".into(), Value::new(3));
    if let Some(a) = m.get_mut(&"a".to_owned()) {
        a.set(10);
    }
    m.clear();
    assert!(m.is_empty());
    let change: BTreeMap<String, MapOp<Value<i32>, i32>> = decode(&encode(&Draft(&m))?)?;
    let want = BTreeMap::from([
        ("a".to_owned(), MapOp::Remove),
        ("b".to_owned(), MapOp::Remove),
    ]);
    assert_eq!(change, want);
    m.rollback();
    assert_eq!(encode(&m)?, snapshot);
    assert!(!m.has_draft());
    Ok(())
}

/// A list change is the cut, the pushed items, then edits; an item pushed and edited is one
/// push. Pins the postcard bytes of one change.
#[test]
fn list_change_truncate_push_edits() -> Res {
    let mut l: List<Value<i32>> = (0..4).map(Value::new).collect();
    for (i, v) in [(1, 10), (3, 30)] {
        if let Some(item) = l.get_mut(i) {
            item.set(v);
        }
    }
    l.push(Value::new(4));
    l.truncate(3);
    l.push(Value::new(5));
    if let Some(item) = l.get_mut(3) {
        item.set(50);
    }
    assert_eq!(
        l.iter().map(|v| *v.get()).collect::<Vec<_>>(),
        [0, 10, 2, 50]
    );
    let bytes = encode(&Draft(&l))?;
    let change: ListChange<Value<i32>, i32> = decode(&bytes)?;
    let want = ListChange {
        truncate: Some(3),
        push: vec![Value::new(50)],
        edits: vec![(1, 10)],
    };
    assert_eq!(change, want);
    // Some(3) | push len 1, zigzag(50) | edits len 1, (1, zigzag(10)).
    assert_eq!(bytes, [1, 3, 1, 100, 1, 1, 20]);
    l.rollback();
    assert_eq!(l.iter().map(|v| *v.get()).collect::<Vec<_>>(), [0, 1, 2, 3]);
    l.push(Value::new(4));
    assert!(l.pop());
    assert!(!l.has_draft(), "pushed and popped: no change");

    let mut replica: List<Value<i32>> = List::new();
    let bad = ListChange {
        truncate: Some(1),
        push: vec![],
        edits: vec![],
    };
    assert_eq!(replica.apply(bad), Err(ApplyError::OutOfRange));
    Ok(())
}

/// `clear` cuts the list to nothing; rollback gives the items back; a replica follows.
#[test]
fn list_clear() -> Res {
    let mut l: List<Value<i32>> = (0..3).map(Value::new).collect();
    l.push(Value::new(3));
    l.clear();
    assert!(l.is_empty());
    let bytes = encode(&Draft(&l))?;
    let want: ListChange<Value<i32>, i32> = ListChange {
        truncate: Some(0),
        push: vec![],
        edits: vec![],
    };
    assert_eq!(decode::<ListChange<Value<i32>, i32>>(&bytes)?, want);
    let mut replica: List<Value<i32>> = (0..3).map(Value::new).collect();
    assert_eq!(replica.apply(decode(&bytes)?), Ok(()));
    assert!(replica.is_empty());
    l.rollback();
    assert_eq!(l.iter().map(|v| *v.get()).collect::<Vec<_>>(), [0, 1, 2]);
    l.clear();
    l.commit();
    assert!(l.is_empty() && !l.has_draft());
    Ok(())
}

/// A derived change has one optional entry per field, in field order, every one encoded.
#[test]
fn derive_change_per_field() -> Res {
    let mut s = Shape::default();
    assert!(!s.has_draft());
    s.tags.insert("on".into(), Value::new(true));
    s.x.set(7);
    let change: ShapeChange = decode(&encode(&Draft(&s))?)?;
    assert_eq!(change.x, Some(7));
    assert!(change.label.is_none() && change.points.is_none());
    let tags = change.tags.unwrap_or_default();
    assert_eq!(
        tags,
        BTreeMap::from([("on".to_owned(), MapOp::Set(Value::new(true)))])
    );
    let label_only = ShapeChange {
        x: None,
        label: Some("hi".into()),
        points: None,
        tags: None,
    };
    let change: ShapeChange = decode(&encode(&label_only)?)?;
    assert!(change.x.is_none());
    s.commit();
    assert_eq!(s.apply(change), Ok(()));
    assert_eq!(s.label.get(), "hi");
    Ok(())
}

/// Setting the committed value again clears the draft.
#[test]
fn value_set_back_clears_draft() {
    let mut v = Value::new(1);
    v.set(1);
    assert!(!v.has_draft(), "equal set is no change");
    v.set(2);
    v.set(3);
    assert!(v.has_draft());
    v.set(1);
    assert!(!v.has_draft(), "back to the committed value");
    assert_eq!(*v.get(), 1);
}

/// A new node's `rev` is above every one given out before, so a node replaced by an equal one
/// still reads as changed; a derived struct's `rev` grows with any field's; a container's covers
/// its items (`get_mut`).
#[test]
fn rev_of_new_nodes_structs_and_containers() {
    let old = Value::new(0);
    let new = Value::new(0);
    assert!(new.rev() > old.rev());

    let mut s = Shape::default();
    let r = s.rev();
    s.label.set("hi".into());
    assert!(s.rev() > r);
    let r = s.rev();
    s.commit();
    assert_eq!(s.rev(), r, "commit changes nothing shown");

    let mut tags: Map<String, Value<bool>> = Map::new();
    tags.insert("a".into(), Value::new(false));
    tags.commit();
    let (map, item) = (tags.rev(), tags.get("a").map(Synced::rev));
    if let Some(v) = tags.get_mut(&"a".to_owned()) {
        v.set(true);
    }
    assert!(tags.rev() > map);
    assert!(tags.get("a").map(Synced::rev) > item);
    let r = tags.rev();
    tags.insert("a".into(), Value::new(true));
    assert!(
        tags.rev() > r && tags.get("a").map(Synced::rev) > item,
        "a replaced item reads as changed"
    );
}

/// A snapshot or change with bytes left over or missing does not decode.
#[test]
fn truncated_or_padded_change_does_not_decode() -> Res {
    let mut s = Shape::default();
    s.x.set(1);
    let mut bytes = encode(&Draft(&s))?;
    bytes.push(0);
    assert!(matches!(
        decode::<ShapeChange>(&bytes),
        Err(ApplyError::Decode)
    ));
    bytes.truncate(bytes.len().saturating_sub(2));
    assert!(matches!(
        decode::<ShapeChange>(&bytes),
        Err(ApplyError::Decode)
    ));
    Ok(())
}

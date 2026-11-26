//! Property tests for [`List`] and [`Map`] against independent models (`.rules/verification.md`
//! rule 6): a plain `Vec` / `BTreeMap` receives the same ops. Each round runs random ops, then
//! commits or rolls back. After every round the source equals the draft model (or, after a
//! rollback, the committed model), and a fresh replica that only ever applied the decoded
//! postcard changes equals the committed model.
//!
//! The RNG seed is fixed in the config, so runs are deterministic. `PROPTEST_RNG_SEED` does not
//! override a fixed config seed; change [`SEED`] to explore.

use std::collections::BTreeMap;

use lattice_state::{Draft, List, Map, Synced, Value, decode, encode};
use proptest::prelude::*;
use proptest::test_runner::RngSeed;

/// Fixed proptest seed.
const SEED: u64 = 0x1A77_1CE5_7A7E;

fn config() -> ProptestConfig {
    ProptestConfig {
        cases: 512,
        rng_seed: RngSeed::Fixed(SEED),
        ..ProptestConfig::default()
    }
}

/// Encodes the draft, commits, decodes and applies it on `replica`. Checks law c on the way.
fn ship<T: Synced>(source: &mut T, replica: &mut T) -> Result<(), TestCaseError> {
    if !source.has_draft() {
        source.commit();
        return Ok(());
    }
    let bytes = encode(&Draft(&*source)).map_err(|e| TestCaseError::fail(e.to_string()))?;
    source.commit();
    let change: T::Change = decode(&bytes).map_err(|e| TestCaseError::fail(e.to_string()))?;
    let again = encode(&change).map_err(|e| TestCaseError::fail(e.to_string()))?;
    prop_assert_eq!(again, bytes, "decode then encode gives the same bytes");
    replica
        .apply(change)
        .map_err(|e| TestCaseError::fail(e.to_string()))
}

/// A fresh replica from the postcard snapshot of `source`.
fn replica_of<T: Synced>(source: &T) -> Result<T, TestCaseError> {
    let bytes = encode(source).map_err(|e| TestCaseError::fail(e.to_string()))?;
    decode(&bytes).map_err(|e| TestCaseError::fail(e.to_string()))
}

#[derive(Debug, Clone)]
enum ListOp {
    Push(i32),
    Truncate(usize),
    Pop,
    Set(usize, i32),
    Clear,
}

fn list_op() -> impl Strategy<Value = ListOp> {
    prop_oneof![
        3 => any::<i32>().prop_map(ListOp::Push),
        1 => (0usize..8).prop_map(ListOp::Truncate),
        1 => Just(ListOp::Pop),
        3 => (0usize..8, any::<i32>()).prop_map(|(i, v)| ListOp::Set(i, v)),
        1 => Just(ListOp::Clear),
    ]
}

fn list_run(op: &ListOp, list: &mut List<Value<i32>>, model: &mut Vec<i32>) {
    match *op {
        ListOp::Push(v) => {
            list.push(Value::new(v));
            model.push(v);
        }
        ListOp::Truncate(len) => {
            list.truncate(len);
            model.truncate(len);
        }
        ListOp::Pop => {
            assert_eq!(list.pop(), model.pop().is_some());
        }
        ListOp::Set(i, v) => {
            if let Some(item) = list.get_mut(i) {
                item.set(v);
            }
            if let Some(slot) = model.get_mut(i) {
                *slot = v;
            }
        }
        ListOp::Clear => {
            list.clear();
            model.clear();
        }
    }
}

fn list_items(list: &List<Value<i32>>) -> Vec<i32> {
    list.iter().map(|v| *v.get()).collect()
}

#[derive(Debug, Clone)]
enum MapOpCase {
    Insert(u8, i32),
    Remove(u8),
    Set(u8, i32),
    Entry(u8, i32),
    Clear,
}

fn map_op() -> impl Strategy<Value = MapOpCase> {
    let key = 0u8..8;
    prop_oneof![
        3 => (key.clone(), any::<i32>()).prop_map(|(k, v)| MapOpCase::Insert(k, v)),
        2 => key.clone().prop_map(MapOpCase::Remove),
        3 => (key.clone(), any::<i32>()).prop_map(|(k, v)| MapOpCase::Set(k, v)),
        2 => (key, any::<i32>()).prop_map(|(k, v)| MapOpCase::Entry(k, v)),
        1 => Just(MapOpCase::Clear),
    ]
}

fn map_run(op: &MapOpCase, map: &mut Map<u8, Value<i32>>, model: &mut BTreeMap<u8, i32>) {
    match *op {
        MapOpCase::Insert(k, v) => {
            map.insert(k, Value::new(v));
            model.insert(k, v);
        }
        MapOpCase::Remove(k) => {
            assert_eq!(map.remove(&k), model.remove(&k).is_some());
        }
        MapOpCase::Set(k, v) => {
            if let Some(item) = map.get_mut(&k) {
                item.set(v);
            }
            if let Some(slot) = model.get_mut(&k) {
                *slot = v;
            }
        }
        MapOpCase::Entry(k, v) => {
            map.entry(k).set(v);
            model.insert(k, v);
        }
        MapOpCase::Clear => {
            map.clear();
            model.clear();
        }
    }
}

fn map_items(map: &Map<u8, Value<i32>>) -> BTreeMap<u8, i32> {
    map.iter().map(|(k, v)| (*k, *v.get())).collect()
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn list_replica_follows_model(
        start in prop::collection::vec(any::<i32>(), 0..6),
        rounds in prop::collection::vec((prop::collection::vec(list_op(), 0..8), any::<bool>()), 1..8),
    ) {
        let mut source: List<Value<i32>> = start.iter().copied().map(Value::new).collect();
        let mut replica = replica_of(&source)?;
        let mut committed = start;
        for (ops, commit) in &rounds {
            let mut draft = committed.clone();
            for op in ops {
                list_run(op, &mut source, &mut draft);
            }
            prop_assert_eq!(list_items(&source), draft.clone());
            if *commit {
                ship(&mut source, &mut replica)?;
                committed = draft;
            } else {
                source.rollback();
            }
            prop_assert!(!source.has_draft());
            prop_assert_eq!(list_items(&source), committed.clone());
            prop_assert_eq!(list_items(&replica), committed.clone());
        }
    }

    #[test]
    fn map_replica_follows_model(
        start in prop::collection::btree_map(0u8..8, any::<i32>(), 0..6),
        rounds in prop::collection::vec((prop::collection::vec(map_op(), 0..8), any::<bool>()), 1..8),
    ) {
        let mut source: Map<u8, Value<i32>> = start.iter().map(|(k, v)| (*k, Value::new(*v))).collect();
        let mut replica = replica_of(&source)?;
        let mut committed = start;
        for (ops, commit) in &rounds {
            let mut draft = committed.clone();
            for op in ops {
                map_run(op, &mut source, &mut draft);
            }
            prop_assert_eq!(map_items(&source), draft.clone());
            if *commit {
                ship(&mut source, &mut replica)?;
                committed = draft;
            } else {
                source.rollback();
            }
            prop_assert!(!source.has_draft());
            prop_assert_eq!(map_items(&source), committed.clone());
            prop_assert_eq!(map_items(&replica), committed.clone());
        }
    }
}

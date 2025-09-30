use std::{
    collections::{HashMap, HashSet, hash_map::Drain},
    hash::Hash,
    iter::Map,
    marker::PhantomData,
};

use crate::sync::CancellationToken;

mod persist {
    use std::marker::PhantomData;

    pub trait StatePersistence {
        type State;

        /// Load the persisted state. This should be a highly tolerant method; if state is corrupted
        /// it is better to reset than to return an error because the application will not be able
        /// to resolve an error and will not be able to run.
        #[allow(async_fn_in_trait)]
        async fn load() -> Result<Self::State, Box<dyn std::error::Error>>;

        /// Persist the state for loading later by `load`.
        #[allow(async_fn_in_trait)]
        async fn save(&self, state: &Self::State) -> Result<(), Box<dyn std::error::Error>>;

        /// Whether the application should reload the state between iterations or may maintain
        /// a cached copy in memory.
        fn retain() -> bool;
    }

    pub struct InMemoryPersistence<T> {
        _phantom: PhantomData<T>,
    }

    impl<T> Default for InMemoryPersistence<T>
    where
        T: Default,
    {
        fn default() -> Self {
            Self {
                _phantom: std::marker::PhantomData,
            }
        }
    }

    impl<T> StatePersistence for InMemoryPersistence<T>
    where
        T: Default,
    {
        type State = T;

        async fn load() -> Result<Self::State, Box<dyn std::error::Error>> {
            let state = T::default();
            Ok(state)
        }

        async fn save(&self, _state: &Self::State) -> Result<(), Box<dyn std::error::Error>> {
            Ok(())
        }

        fn retain() -> bool {
            true
        }
    }
}

#[cfg(test)]
mod test_persistence {
    use std::error::Error;

    use crate::state::{StatePersistence, persist::InMemoryPersistence};

    #[tokio::test]
    async fn load_returns_default_value() -> Result<(), Box<dyn Error>> {
        let state = <InMemoryPersistence<i32> as StatePersistence>::load().await?;

        assert_eq!(0, state);

        Ok(())
    }

    #[test]
    fn retain_true() {
        let retain = <InMemoryPersistence<i32> as StatePersistence>::retain();

        assert!(retain);
    }

    #[tokio::test]
    async fn persist_noop() -> Result<(), Box<dyn Error>> {
        let persistence = InMemoryPersistence::<i32>::default();

        // Basically we are looking for it to return Ok()
        persistence.save(&0).await
    }
}

pub use persist::*;

mod state_change {
    use std::collections::{HashMap, HashSet};

    #[derive(Debug)]
    pub enum StateChange<Key> {
        New(Key),
        Update(Key),
        Delete(Key),
    }

    #[derive(Debug)]
    pub enum NotifiedState<Key> {
        None(Key),
        New(Key),
        Update(Key),
        Delete(Key),
    }

    pub trait TableState {
        type Key;
        type Hash;

        /// Notifies the state that the key is present, and has the provided hash.
        /// Internally, the state shall record the change type from its internal
        /// state of the key.
        fn set_row(&mut self, key: Self::Key, hash: Self::Hash);

        /// Consumes the state and produces the change set. This change set should be merged into
        /// persistence and notified to the message bus.
        /// `delete_remainder` determines if anything not passed to `set_presence` should be
        /// considered deleted. This flag should be set if the state was able to be updated
        /// fully, and should be `false` if the change detector was stopped prematurely.
        fn drain(self, delete_remainder: bool) -> impl Iterator<Item = StateChange<Self::Key>>;
    }

    #[derive(Debug)]
    pub struct DefaultTableState<Key, Hash> {
        tablehash: Option<u64>,
        rows: HashMap<Key, Hash>,
        changes: Vec<NotifiedState<Key>>,
    }

    impl<Key, Hash> DefaultTableState<Key, Hash> {
        pub fn new(tablehash: Option<u64>, rows: HashMap<Key, Hash>) -> Self {
            Self {
                tablehash,
                rows,
                changes: vec![],
            }
        }
    }

    impl<Key, Hash> Default for DefaultTableState<Key, Hash> {
        fn default() -> Self {
            Self::new(None, HashMap::new())
        }
    }

    impl<Key, Hash> TableState for DefaultTableState<Key, Hash>
    where
        Key: Eq + std::hash::Hash + Clone,
        Hash: Eq,
    {
        type Key = Key;
        type Hash = Hash;

        fn set_row(&mut self, key: Self::Key, hash: Self::Hash) {
            if let Some(value) = self.rows.get_mut(&key) {
                if value == &hash {
                    self.changes.push(NotifiedState::None(key));
                } else {
                    *value = hash;
                    self.changes.push(NotifiedState::Update(key));
                }
            } else {
                self.changes.push(NotifiedState::New(key.clone()));
                self.rows.insert(key, hash);
            }
        }

        fn drain(self, delete_remainder: bool) -> impl Iterator<Item = StateChange<Self::Key>> {
            // For each item in self.rows, check for a change in self.changes.
            // If there is no change and delete_remainder = true, produce a Delete
            // If there is a change, map it to the proper change type
            // Then drain the rest of the changes (which should all be inserts at this point)
            // and publish them also

            let mut changes = Vec::new();
            let mut unseen = HashSet::new();

            if delete_remainder {
                for key in self.rows.keys() {
                    unseen.insert(key);
                }
            }

            for seen in self.changes {
                match seen {
                    NotifiedState::Delete(k) => {
                        unseen.remove(&k);
                        changes.push(StateChange::Delete(k))
                    }
                    NotifiedState::New(k) => {
                        unseen.remove(&k);
                        changes.push(StateChange::New(k));
                    }
                    NotifiedState::Update(k) => {
                        unseen.remove(&k);
                        changes.push(StateChange::Update(k));
                    }
                    NotifiedState::None(k) => {
                        unseen.remove(&k);
                    }
                }
            }

            for rem in unseen {
                changes.push(StateChange::Delete(rem.to_owned()));
            }

            changes.into_iter()
        }
    }
}

#[cfg(test)]
mod test_state_change {
    use super::state_change::*;
    use std::collections::HashMap;

    #[test]
    fn drain_new() {
        let mut ts = DefaultTableState::<i32, i32>::default();

        ts.set_row(1, 11);

        let drain: Vec<_> = ts.drain(true).collect();

        assert_eq!(1, drain.len());
        match drain.get(0).unwrap() {
            StateChange::New(_) => {}
            or => panic!("The wrong state change type was discovered. {:?}", or),
        }
    }

    #[test]
    fn drain_delete_no_remainder() {
        let mut hash = HashMap::new();
        hash.insert(1, 31);
        let ts = DefaultTableState::new(None, hash);

        let drain: Vec<_> = ts.drain(false).collect();

        assert!(drain.is_empty());
    }

    #[test]
    fn drain_delete_remainder() {
        let mut hash = HashMap::new();
        hash.insert(1, 31);
        let ts = DefaultTableState::new(None, hash);

        let drain: Vec<_> = ts.drain(true).collect();

        assert_eq!(1, drain.len());
        match drain.get(0).unwrap() {
            StateChange::Delete(id) => assert_eq!(1, *id),
            or => panic!("Expected a Delete but got {:?}", or),
        }
    }

    #[test]
    fn drain_update() {
        let mut hash = HashMap::new();
        hash.insert(1, 31);
        let mut ts = DefaultTableState::new(None, hash);
        ts.set_row(1, 90);

        let drain: Vec<_> = ts.drain(true).collect();

        assert_eq!(1, drain.len());
        match drain.get(0).unwrap() {
            StateChange::Update(id) => assert_eq!(1, *id),
            or => panic!("Expected an Update but got {:?}", or),
        }
    }

    #[test]
    fn drain_set_to_same_value() {
        let mut hash = HashMap::new();
        hash.insert(1, 31);
        let mut ts = DefaultTableState::new(None, hash);
        ts.set_row(1, 31);

        let drain: Vec<_> = ts.drain(true).collect();

        assert_eq!(0, drain.len());
    }
}

pub use state_change::*;

/// State of a singular row, compared with the computed states to see if there was a change.
pub struct RowState {
    id: u64,
    hash: u64,
}

impl RowState {
    pub fn new(id: u64, hash: u64) -> Self {
        Self { id, hash }
    }

    pub fn deconstruct(self) -> (u64, u64) {
        (self.id, self.hash)
    }
}

/// State of a set of data, first the tablehash is compared and if different, then the rowhash is
/// compared to see if there was a change. If there is no way to determine a tablehash, the value
/// should be set to 0 and the `ChangeDetector` should return a nonzero value.
pub struct State {
    /// Hash of the entire set.
    tablehash: Option<u64>,

    /// Map produced from from `RowState`. Maps id to hash.
    rowhash: HashMap<u64, u64>,
}

impl State {
    pub fn empty() -> Self {
        Self {
            tablehash: None,
            rowhash: HashMap::new(),
        }
    }

    pub fn table(&self) -> Option<u64> {
        self.tablehash
    }

    pub fn row(&self, id: u64) -> Option<u64> {
        self.rowhash.get(&id).copied()
    }

    pub fn pop_row(&mut self, id: u64) -> Option<u64> {
        self.rowhash.remove(&id)
    }

    pub fn drain(&mut self) -> Drain<'_, u64, u64> {
        self.rowhash.drain()
    }

    pub fn set_row(&mut self, id: u64, hash: u64) -> () {
        self.rowhash.insert(id, hash);
    }
}

pub trait ChangeDetector {
    type Change;

    /// Produces a hash of the entire observed set. If the change detector cannot reasonably
    /// hash the entire set, it should return None.
    async fn tablehash(&self, cancel: &CancellationToken) -> Option<u64>;

    /// Produces the change set from state. It does not need to modify `State` as the engine will
    /// handle updating each row. The returned `Vec` must consist of (rowid, hash, messagebody).
    /// If `cancel` is triggered during this call, return early with the changes that are known.
    async fn rowhash(
        &self,
        state: &State,
        cancel: &CancellationToken,
    ) -> Vec<(RowState, Option<Self::Change>)>;
}

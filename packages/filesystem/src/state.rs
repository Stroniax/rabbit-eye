use std::{
    collections::{HashMap, hash_map::Drain},
    hash::Hash,
    marker::PhantomData,
};

use crate::sync::CancellationToken;

pub trait StatePersistence {
    type State;

    /// Load the persisted state. This should be a highly tolerant method; if state is corrupted
    /// it is better to reset than to return an error because the application will not be able
    /// to resolve an error and will not be able to run.
    async fn load() -> Result<Self::State, Box<dyn std::error::Error>>;

    /// Persist the state for loading later by `load`.
    async fn save(self, state: &Self::State) -> Result<(), Box<dyn std::error::Error>>;

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

    async fn save(self, _state: &Self::State) -> Result<(), Box<dyn std::error::Error>> {
        Ok(())
    }

    fn retain() -> bool {
        true
    }
}

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

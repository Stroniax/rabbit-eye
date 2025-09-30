use std::collections::HashMap;

use crate::sync::CancellationToken;

// This will have logic for tracking the state of the data that is polled.

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
    ) -> Vec<(RowState, Self::Change)>;
}

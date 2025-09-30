// This will have logic for tracking the state of the data that is polled.

/// State of a singular row, compared with the computed states to see if there was a change.
struct RowState {
    id: usize,
    hash: usize,
}

/// State of a set of data, first the tablehash is compared and if different, then the rowhash is
/// compared to see if there was a change. If there is no way to determine a tablehash, the value
/// should be set to 0 and the `ChangeDetector` should return a nonzero value.
struct State {
    tablehash: Option<usize>,
    rowhash: Vec<RowState>,
}

use std::path::PathBuf;
use tokio_util::sync::CancellationToken;

trait ChangeDetector {
    type Change;

    /// Produces a hash of the entire observed set. If the change detector cannot reasonably
    /// hash the entire set, it should return None.
    async fn tablehash(&self, cancel: &CancellationToken) -> Option<usize>;

    /// Produces the change set from state. It does not need to modify `State` as the engine will
    /// handle updating each row. The returned `Vec` must consist of (rowid, hash, messagebody).
    /// If `cancel` is triggered during this call, return early with the changes that are known.
    async fn rowhash(
        &self,
        state: &State,
        cancel: &CancellationToken,
    ) -> Vec<(usize, usize, Self::Change)>;
}

struct FileChangeDetector {
    root: PathBuf,
}

impl ChangeDetector for FileChangeDetector {
    type Change = FileChange;

    async fn tablehash(&self, _cancel: &CancellationToken) -> Option<usize> {
        None
    }

    async fn rowhash(
        &self,
        _state: &State,
        _cancel: &CancellationToken,
    ) -> Vec<(usize, usize, Self::Change)> {
        vec![]
    }
}

struct FileChange {
    path: String,
}

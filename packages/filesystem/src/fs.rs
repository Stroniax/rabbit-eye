use crate::state::{ChangeDetector, RowState, State};
use crate::sync::CancellationToken;
use amqprs::{
    BasicProperties,
    channel::{BasicPublishArguments, Channel},
};
use std::hash::{Hash, Hasher};
use std::{error::Error, os::windows::fs::MetadataExt, path::PathBuf};

pub async fn check_and_report_files(
    channel: &Channel,
    cancel: &CancellationToken,
    state: &mut State,
) -> Result<(), Box<dyn Error>> {
    let root = PathBuf::from(std::env::current_dir()?);
    let changedetector = FileChangeDetector::new(root)
        .with_recursive(true)
        .with_child_changes(true)
        .build();

    if let Some(tablehash) = changedetector.tablehash(&cancel).await
        && state.table().map_or_else(|| false, |t| t == tablehash)
    {
        eprintln!("The whole table is the same.");
        return Ok(());
    }

    let changes = changedetector.rowhash(&state, &cancel).await;

    eprintln!("There are {} pending changes.", changes.len());

    for (row, message) in changes {
        let (id, hash) = row.deconstruct();
        state.set_row(id, hash);

        let publish_args = BasicPublishArguments::new("", "rabbit-eye-dev");
        channel
            .basic_publish(
                BasicProperties::default(),
                message.path.into_bytes(),
                publish_args,
            )
            .await?;
    }

    Ok(())
}

pub struct FileChange {
    pub path: String,
    pub last_write_utc: u64,
}

#[derive(Clone)]
pub struct FileChangeDetector {
    /// The root directory to begin inspection.
    root: PathBuf,
    /// Also check the directories within any given directory.
    recursive: bool,
    /// Consider a directory as modified if a child of the directory was modified.
    include_child_changes: bool,
}

impl FileChangeDetector {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            recursive: false,
            include_child_changes: false,
        }
    }

    pub fn with_recursive(&mut self, recursive: bool) -> &mut Self {
        self.recursive = recursive;
        self
    }

    pub fn with_child_changes(&mut self, child_changes: bool) -> &mut Self {
        self.include_child_changes = child_changes;
        self
    }

    pub fn build(&self) -> Self {
        self.clone()
    }
}

struct EPrintLnDrop {
    message: String,
}
impl Drop for EPrintLnDrop {
    fn drop(&mut self) {
        eprintln!("{}", self.message)
    }
}

impl ChangeDetector for FileChangeDetector {
    type Change = FileChange;

    async fn tablehash(&self, _cancel: &CancellationToken) -> Option<u64> {
        None
    }

    async fn rowhash(
        &self,
        state: &State,
        cancel: &CancellationToken,
    ) -> Vec<(RowState, Self::Change)> {
        let _drop = EPrintLnDrop {
            message: String::from("The row hash was cancelled, aborted, or gracefully ended."),
        };

        let mut dir = vec![self.root.clone()];

        let mut changes = Vec::new();
        let mut check_count = 0;

        while let Some(root) = dir.pop() {
            if cancel.is_cancelled() {
                eprintln!("The row hash was cancelled.");
                break;
            }

            let mut dir_files = tokio::fs::read_dir(&root).await.unwrap();
            while let Some(file) = dir_files.next_entry().await.unwrap() {
                if cancel.is_cancelled() {
                    eprintln!("The row hash was cancelled.");
                    break;
                }

                let path = file.file_name().into_string().unwrap();
                let metadata = file.metadata().await.unwrap();

                if self.recursive && metadata.is_dir() {
                    let other = root.join(file.file_name());
                    dir.push(other);
                }

                let mut hasher = std::hash::DefaultHasher::new();
                path.hash(&mut hasher);
                let file_hash = hasher.finish();
                let change_hash = metadata.last_write_time();
                let row = RowState::new(file_hash, change_hash);
                check_count += 1;

                if check_count % 1_000 == 0 {
                    eprintln!("Processing... {}", check_count);
                }

                if let Some(stored_hash) = state.row(file_hash)
                    && stored_hash == change_hash
                {
                    // eprintln!("Not modified: {:?}", root.join(file.file_name()));
                    continue;
                }

                eprintln!("Modified    : {:?}", root.join(file.file_name()));

                let file_change = FileChange {
                    path,
                    last_write_utc: metadata.last_write_time(),
                };

                changes.push((row, file_change));
            }
        }

        eprintln!("Changes: {} / {}.", changes.len(), check_count);
        changes
    }
}

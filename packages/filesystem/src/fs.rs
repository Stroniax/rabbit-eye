use crate::sync::CancellationToken;
use amqprs::{
    BasicProperties,
    channel::{BasicPublishArguments, Channel},
};
use rabbit_eye::state::{
    ChangeDetector, ChangeDetectorResult, DefaultTableState, StateChange, TableState,
};
use std::hash::{Hash, Hasher};
use std::{error::Error, os::windows::fs::MetadataExt, path::PathBuf};

pub async fn check_and_report_files(
    channel: &Channel,
    cancel: &CancellationToken,
    state: &mut impl TableState<String, u64>,
) -> Result<(), Box<dyn Error>> {
    let root = PathBuf::from(std::env::current_dir()?);
    let mut changedetector = FileChangeDetector::new(root)
        .with_recursive(true)
        .with_child_changes(true)
        .build();

    if let Some(former) = state.tablehash()
        && let Some(current) = changedetector.tablehash(&cancel).await
        && former == current
    {
        return Ok(());
    }

    let changes = changedetector.rowhash(&mut *state, &cancel).await;

    let delete_remainder = match changes {
        ChangeDetectorResult::Aborted => return Ok(()),
        ChangeDetectorResult::Cancelled => false,
        ChangeDetectorResult::DeleteRemainder => true,
        ChangeDetectorResult::Faulted(_) => false,
    };

    for change in state.drain(delete_remainder) {
        match change {
            StateChange::New(val) | StateChange::Update(val) | StateChange::Delete(val) => {
                let publish_args = BasicPublishArguments::new("", "rabbit-eye-dev");
                channel
                    .basic_publish(BasicProperties::default(), val.into_bytes(), publish_args)
                    .await?;
            }
        }
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

impl ChangeDetector for FileChangeDetector {
    type Key = String;
    type Hash = u64;

    async fn tablehash(&mut self, _cancel: &CancellationToken) -> Option<u64> {
        None
    }

    async fn rowhash(
        self,
        state: &mut impl TableState<Self::Key, Self::Hash>,
        cancel: &CancellationToken,
    ) -> ChangeDetectorResult {
        let mut dir = vec![self.root.clone()];

        while let Some(root) = dir.pop() {
            if cancel.is_cancelled() {
                eprintln!("The row hash was cancelled.");
                return ChangeDetectorResult::Cancelled;
            }

            let mut dir_files = tokio::fs::read_dir(&root).await.unwrap();
            while let Some(file) = dir_files.next_entry().await.unwrap() {
                if cancel.is_cancelled() {
                    eprintln!("The row hash was cancelled.");
                    return ChangeDetectorResult::Cancelled;
                }

                let metadata = file.metadata().await.unwrap();

                let full_name = root.join(file.file_name());
                if self.recursive && metadata.is_dir() {
                    dir.push(full_name.clone());
                }

                let change_hash = metadata.last_write_time();

                state.set_row(full_name.display().to_string(), change_hash);
            }
        }

        ChangeDetectorResult::DeleteRemainder
    }
}

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
    let root = PathBuf::from("C:");
    let changedetector = FileChangeDetector::new(root);

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

pub struct FileChangeDetector {
    root: PathBuf,
}

impl FileChangeDetector {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
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
        _cancel: &CancellationToken,
    ) -> Vec<(RowState, Self::Change)> {
        let mut dir_files = tokio::fs::read_dir(&self.root).await.unwrap();
        let mut changes = Vec::new();
        while let Some(file) = dir_files.next_entry().await.unwrap() {
            let path = file.file_name().into_string().unwrap();
            let metadata = file.metadata().await.unwrap();

            let mut hasher = std::hash::DefaultHasher::new();
            path.hash(&mut hasher);
            let file_hash = hasher.finish();
            let change_hash = metadata.last_write_time();
            let row = RowState::new(file_hash, change_hash);

            if let Some(stored_hash) = state.row(file_hash)
                && stored_hash == change_hash
            {
                eprintln!(
                    "The file {:?} is already in the hash... there is nothing new under the sun.",
                    path
                );
                continue;
            }

            eprintln!("The file {:?} has been modified!", path);

            let file_change = FileChange {
                path,
                last_write_utc: metadata.last_write_time(),
            };

            changes.push((row, file_change));
        }

        changes
    }
}

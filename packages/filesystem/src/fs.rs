use std::{error::Error, os::windows::fs::MetadataExt};

use amqprs::{
    BasicProperties,
    channel::{BasicPublishArguments, Channel},
};

pub async fn check_and_report_files(channel: &Channel) -> Result<(), Box<dyn Error>> {
    let mut root = tokio::fs::read_dir("C:").await?;
    while let Some(dir_or_file) = root.next_entry().await? {
        let metadata = dir_or_file.metadata().await?;

        println!(
            "File {:?} last written to at {}",
            dir_or_file.file_name(),
            metadata.last_write_time()
        );
    }

    let publish_args = BasicPublishArguments::new("", "rabbit-eye-dev");
    channel
        .basic_publish(BasicProperties::default(), vec![], publish_args)
        .await?;

    Ok(())
}

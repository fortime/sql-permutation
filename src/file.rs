use anyhow::Result;
use log::{log_enabled, Level::Debug};
use std::path::Path;
use tokio::fs::File;
use tokio::io::BufReader;
use tokio::prelude::*;

pub async fn read_sqls_by_line(file: impl AsRef<Path>) -> Result<Vec<String>> {
    let mut path = None;
    if log_enabled!(Debug) {
        path = Some(String::from(file.as_ref().to_string_lossy()));
    }
    let mut lines = BufReader::new(File::open(file).await?).lines();
    let mut result = vec![];
    while let Some(line) = lines.next_line().await? {
        result.push(line);
    }
    if log_enabled!(Debug) {
        log::debug!("read from file by line[{:?}]:\n{:?}", path, result);
    }
    Ok(result)
}

pub async fn read_sqls(file: impl AsRef<Path>) -> Result<String> {
    let mut result = vec![];
    let mut path = None;
    if log_enabled!(Debug) {
        path = Some(String::from(file.as_ref().to_string_lossy()));
    }
    File::open(file).await?.read_to_end(&mut result).await?;
    if log_enabled!(Debug) {
        log::debug!(
            "read from file[{:?}]:\n{}",
            path,
            String::from_utf8_lossy(&result)
        );
    }
    Ok(String::from_utf8(result)?)
}

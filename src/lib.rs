use anyhow::Result;
use std::path::Path;

pub mod algo;
pub mod arg;
pub mod concurrent;
pub mod file;

pub fn init_log(log_config_file: Option<impl AsRef<Path>>) -> Result<()> {
    match log_config_file {
        Some(path) => {
            log4rs::init_file(path, Default::default())?;
        }
        None => {}
    }
    Ok(())
}

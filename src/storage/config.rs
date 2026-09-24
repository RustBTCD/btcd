use std::path::PathBuf;

use serde::Deserialize;

use super::engine::RocksOptions;

/// `[storage]` section of the config file. Every field is optional.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct StorageConfig {
    /// Base data directory. Each database lives in a subdirectory of it, and is created if
    /// missing.
    pub path: PathBuf,
    /// Tuning for the engine the node uses. Which tables go in which database is decided in
    /// code, not here.
    pub rocks: RocksOptions,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            path: PathBuf::from("data"),
            rocks: RocksOptions::default(),
        }
    }
}

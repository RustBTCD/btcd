use std::path::PathBuf;

use serde::Deserialize;

/// `[storage]` section of the config file. Every field is optional.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct StorageConfig {
    /// Base data directory. Each chain gets its own database in a subdirectory named after
    /// the chain, for example `data/signet`. Created if missing.
    pub path: PathBuf,
    /// RocksDB block cache shared by all column families, in MiB.
    pub block_cache_mib: usize,
    /// Upper bound on open files. macOS has a default soft limit of 256.
    pub max_open_files: i32,
    /// Values at least this large go to blob files and skip compaction, in bytes.
    pub min_blob_size_bytes: u64,
    /// Number of RocksDB info log files to keep.
    pub keep_log_files: usize,
    /// Threads for flushes and compactions. `None` means the number of CPU cores.
    pub background_threads: Option<usize>,

    // Blob garbage collection frees disk space held by deleted blocks and undo data. RocksDB
    // cannot free part of a blob file: it copies the still-live values out of an old file
    // during compaction, then deletes the whole file.
    /// Share of blob files, counted by number of files and taken oldest first, whose live
    /// values compaction may copy out so the files can be deleted. Range 0.0 to 1.0.
    ///
    /// 0.25 means the oldest quarter of files. 0.0 disables cleanup, so deleted blocks keep
    /// their disk space forever. 1.0 frees all garbage, but compaction may copy every stored
    /// block again.
    pub blob_gc_oldest_files_fraction: f64,
    /// Share of bytes in those oldest files that are garbage at which RocksDB schedules an
    /// extra compaction just to clean them. Range 0.0 to 1.0.
    ///
    /// 1.0 means only when the files are entirely garbage. Below the trigger, cleanup waits
    /// for compactions that run anyway.
    pub blob_gc_garbage_ratio_trigger: f64,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            path: PathBuf::from("data"),
            block_cache_mib: 256,
            max_open_files: 200,
            min_blob_size_bytes: 4096,
            keep_log_files: 4,
            background_threads: None,
            blob_gc_oldest_files_fraction: 0.25,
            blob_gc_garbage_ratio_trigger: 1.0,
        }
    }
}

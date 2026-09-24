//! Engine tuning: what the node configures, and how each table is stored.

use rocksdb::{BlockBasedOptions, Cache, DBCompressionType, Options};
use serde::Deserialize;

use crate::storage::db::Profile;

/// Engine tuning. Defaults suit a node that syncs the chain.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RocksOptions {
    pub block_cache_mib: usize,
    pub max_open_files: i32,
    pub keep_log_files: usize,
    pub background_threads: usize,
    pub min_blob_size_bytes: u64,
    pub blob_gc_oldest_files_fraction: f64,
    pub blob_gc_garbage_ratio_trigger: f64,
}

pub(super) fn table_options(cache: &Cache, profile: Profile, options: &RocksOptions) -> Options {
    let mut table = BlockBasedOptions::default();
    table.set_block_cache(cache);
    if profile == Profile::PointLookup {
        // Ten bits per key gives about one wasted read in a hundred for absent keys.
        table.set_bloom_filter(10.0, false);
    }

    let mut opts = Options::default();
    opts.set_block_based_table_factory(&table);
    opts.set_compression_type(DBCompressionType::Lz4);

    if profile == Profile::LargeValues {
        opts.set_enable_blob_files(true);
        opts.set_min_blob_size(options.min_blob_size_bytes);
        // Without collection, space held by deleted values is never returned.
        opts.set_enable_blob_gc(true);
        opts.set_blob_gc_age_cutoff(options.blob_gc_oldest_files_fraction);
        opts.set_blob_gc_force_threshold(options.blob_gc_garbage_ratio_trigger);
    }
    opts
}

impl Default for RocksOptions {
    fn default() -> Self {
        Self {
            block_cache_mib: 256,
            max_open_files: 200,
            keep_log_files: 4,
            background_threads: 0,
            min_blob_size_bytes: 4096,
            blob_gc_oldest_files_fraction: 0.25,
            blob_gc_garbage_ratio_trigger: 1.0,
        }
    }
}

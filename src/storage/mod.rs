//! Persistent node storage on RocksDB.
//!
//! One database with several column families. Connecting a block touches `utxo`, `undo`,
//! `height_index` and `meta`; keeping them in one database lets that commit atomically.
//!
//! | Column family  | Key                        | Value                                   |
//! |----------------|----------------------------|-----------------------------------------|
//! | `utxo`         | txid ‖ vout (u32 BE)       | [`Coin`]                                |
//! | `undo`         | block hash                 | [`BlockUndo`]                           |
//! | `blocks`       | block hash                 | consensus-encoded block                 |
//! | `headers`      | block hash                 | [`HeaderEntry`]                         |
//! | `height_index` | height (u32 BE)            | block hash, active chain only           |
//! | `meta`         | fixed keys                 | tip hash                                |

// Stores are not wired into the node yet.
#![allow(dead_code, unused_imports)]

mod blocks;
mod chain;
mod coin;
mod config;
mod error;
mod headers;
mod utxo;

use std::sync::{Arc, Mutex};

use rocksdb::{
    BlockBasedOptions, Cache, ColumnFamily, ColumnFamilyDescriptor, DB, DBCompressionType, Options,
};

pub use blocks::BlockStore;
pub use chain::ChainStore;
pub use coin::{BlockUndo, Coin};
pub use config::StorageConfig;
pub use error::{Result, StorageError};
pub use headers::{BlockStatus, HeaderEntry, HeaderStore};
pub use utxo::UtxoStore;

pub(crate) mod cf {
    pub const UTXO: &str = "utxo";
    pub const UNDO: &str = "undo";
    pub const BLOCKS: &str = "blocks";
    pub const HEADERS: &str = "headers";
    pub const HEIGHT_INDEX: &str = "height_index";
    pub const META: &str = "meta";
}

pub(crate) mod meta_key {
    pub const TIP: &[u8] = b"tip";
}

pub(crate) fn cf<'a>(db: &'a DB, name: &'static str) -> Result<&'a ColumnFamily> {
    db.cf_handle(name)
        .ok_or(StorageError::MissingColumnFamily(name))
}

#[derive(Clone)]
pub struct Storage {
    db: Arc<DB>,
    /// Serializes chain writes. Shared by every [`ChainStore`] handed out by this storage.
    chain_lock: Arc<Mutex<()>>,
}

impl Storage {
    pub fn open(config: &StorageConfig) -> Result<Self> {
        let cache = Cache::new_lru_cache(config.block_cache_mib * 1024 * 1024);

        let mut db_opts = Options::default();
        db_opts.create_if_missing(true);
        db_opts.create_missing_column_families(true);
        db_opts.set_max_open_files(config.max_open_files);
        db_opts.set_keep_log_file_num(config.keep_log_files);
        let threads = config
            .background_threads
            .unwrap_or_else(|| std::thread::available_parallelism().map_or(2, |n| n.get()));
        db_opts.increase_parallelism(threads as i32);

        let descriptors = vec![
            ColumnFamilyDescriptor::new(cf::UTXO, point_lookup_options(&cache)),
            ColumnFamilyDescriptor::new(cf::UNDO, blob_options(&cache, config)),
            ColumnFamilyDescriptor::new(cf::BLOCKS, blob_options(&cache, config)),
            ColumnFamilyDescriptor::new(cf::HEADERS, point_lookup_options(&cache)),
            ColumnFamilyDescriptor::new(cf::HEIGHT_INDEX, table_options(&cache)),
            ColumnFamilyDescriptor::new(cf::META, table_options(&cache)),
        ];

        let db = DB::open_cf_descriptors(&db_opts, &config.path, descriptors)?;
        Ok(Self {
            db: Arc::new(db),
            chain_lock: Arc::default(),
        })
    }

    pub fn utxos(&self) -> UtxoStore {
        UtxoStore::new(self.db.clone())
    }

    pub fn blocks(&self) -> BlockStore {
        BlockStore::new(self.db.clone())
    }

    pub fn headers(&self) -> HeaderStore {
        HeaderStore::new(self.db.clone())
    }

    pub fn chain(&self) -> ChainStore {
        ChainStore::new(self.db.clone(), self.chain_lock.clone())
    }
}

fn table_options(cache: &Cache) -> Options {
    let mut table = BlockBasedOptions::default();
    table.set_block_cache(cache);
    let mut opts = Options::default();
    opts.set_block_based_table_factory(&table);
    opts.set_compression_type(DBCompressionType::Lz4);
    opts
}

/// Bloom filters make lookups of missing keys cheap, which matters for UTXO and header checks.
fn point_lookup_options(cache: &Cache) -> Options {
    let mut table = BlockBasedOptions::default();
    table.set_block_cache(cache);
    table.set_bloom_filter(10.0, false);
    let mut opts = Options::default();
    opts.set_block_based_table_factory(&table);
    opts.set_compression_type(DBCompressionType::Lz4);
    opts
}

/// Large values (blocks, undo data) live in blob files so compaction does not rewrite them.
///
/// Garbage collection is on so deleted values eventually free their disk space. See the
/// `blob_gc_*` fields of [`StorageConfig`] for how the two thresholds behave.
fn blob_options(cache: &Cache, config: &StorageConfig) -> Options {
    let mut opts = table_options(cache);
    opts.set_enable_blob_files(true);
    opts.set_min_blob_size(config.min_blob_size_bytes);
    opts.set_enable_blob_gc(true);
    opts.set_blob_gc_age_cutoff(config.blob_gc_oldest_files_fraction);
    opts.set_blob_gc_force_threshold(config.blob_gc_garbage_ratio_trigger);
    opts
}

#[cfg(test)]
pub(crate) mod test_utils {
    use std::path::Path;

    use super::{Storage, StorageConfig};
    use bitcoin::block::{Header, Version as BlockVersion};
    use bitcoin::hashes::Hash;
    use bitcoin::transaction::Version as TxVersion;
    use bitcoin::{
        Amount, Block, BlockHash, CompactTarget, OutPoint, ScriptBuf, Sequence, Transaction, TxIn,
        TxMerkleNode, TxOut, Witness, absolute::LockTime,
    };

    pub fn open_at(path: &Path) -> Storage {
        let config = StorageConfig {
            path: path.to_path_buf(),
            ..StorageConfig::default()
        };
        Storage::open(&config).expect("open storage")
    }

    pub fn open_temp() -> (tempfile::TempDir, Storage) {
        let dir = tempfile::tempdir().expect("temp dir");
        let storage = open_at(dir.path());
        (dir, storage)
    }

    pub fn output(sats: u64) -> TxOut {
        TxOut {
            value: Amount::from_sat(sats),
            script_pubkey: ScriptBuf::from_bytes(vec![0x51]),
        }
    }

    /// `tag` makes otherwise identical coinbases have different txids.
    pub fn coinbase(tag: u8, outputs: Vec<TxOut>) -> Transaction {
        tx(vec![(OutPoint::null(), vec![tag, tag])], outputs)
    }

    pub fn spend(prevouts: &[OutPoint], outputs: Vec<TxOut>) -> Transaction {
        tx(prevouts.iter().map(|p| (*p, vec![])).collect(), outputs)
    }

    fn tx(inputs: Vec<(OutPoint, Vec<u8>)>, output: Vec<TxOut>) -> Transaction {
        Transaction {
            version: TxVersion::TWO,
            lock_time: LockTime::ZERO,
            input: inputs
                .into_iter()
                .map(|(previous_output, sig)| TxIn {
                    previous_output,
                    script_sig: ScriptBuf::from_bytes(sig),
                    sequence: Sequence::MAX,
                    witness: Witness::new(),
                })
                .collect(),
            output,
        }
    }

    pub fn block(prev: BlockHash, nonce: u32, txdata: Vec<Transaction>) -> Block {
        Block {
            header: Header {
                version: BlockVersion::ONE,
                prev_blockhash: prev,
                merkle_root: TxMerkleNode::all_zeros(),
                time: 0,
                bits: CompactTarget::from_consensus(0x207f_ffff),
                nonce,
            },
            txdata,
        }
    }

    pub fn null_hash() -> BlockHash {
        BlockHash::all_zeros()
    }
}

#[cfg(test)]
mod tests {
    use super::test_utils::*;
    use super::*;
    use rocksdb::{BottommostLevelCompaction, CompactOptions};

    /// Live blob bytes and garbage blob bytes in the `blocks` family.
    fn blob_sizes(storage: &Storage) -> (u64, u64) {
        let blocks = cf(&storage.db, cf::BLOCKS).unwrap();
        let prop = |name: &str| {
            storage
                .db
                .property_int_value_cf(blocks, name)
                .unwrap()
                .unwrap()
        };
        (
            prop("rocksdb.live-blob-file-size"),
            prop("rocksdb.live-blob-file-garbage-size"),
        )
    }

    /// Writes two blob files of 20 blocks each, deletes 18 blocks from the older file, and
    /// compacts. Returns blob sizes before the deletes and after the compaction.
    fn delete_and_compact(oldest_files_fraction: f64) -> ((u64, u64), (u64, u64)) {
        let dir = tempfile::tempdir().unwrap();
        let config = StorageConfig {
            path: dir.path().to_path_buf(),
            min_blob_size_bytes: 1,
            blob_gc_oldest_files_fraction: oldest_files_fraction,
            ..StorageConfig::default()
        };
        let storage = Storage::open(&config).unwrap();
        let blocks_cf = cf(&storage.db, cf::BLOCKS).unwrap();
        let blocks = storage.blocks();

        let mut hashes = Vec::new();
        for file in 0..2u32 {
            for i in 0..20u32 {
                let b = block(
                    null_hash(),
                    file * 100 + i,
                    vec![coinbase(0, vec![output(1)])],
                );
                hashes.push(blocks.put(&b).unwrap());
            }
            storage.db.flush_cf(blocks_cf).unwrap();
        }
        let before = blob_sizes(&storage);

        for hash in &hashes[..18] {
            blocks.delete(hash).unwrap();
        }
        storage.db.flush_cf(blocks_cf).unwrap();
        let mut compact = CompactOptions::default();
        compact.set_bottommost_level_compaction(BottommostLevelCompaction::Force);
        storage
            .db
            .compact_range_cf_opt(blocks_cf, None::<&[u8]>, None::<&[u8]>, &compact);

        (before, blob_sizes(&storage))
    }

    #[test]
    fn blob_gc_frees_deleted_blocks() {
        let ((live_before, _), (live_after, garbage_after)) = delete_and_compact(1.0);
        assert!(live_after < live_before, "{live_after} >= {live_before}");
        assert_eq!(garbage_after, 0);
    }

    /// Proves the test above measures the right thing: with cleanup disabled, the space stays.
    #[test]
    fn without_blob_gc_deleted_blocks_keep_disk_space() {
        let ((live_before, _), (live_after, garbage_after)) = delete_and_compact(0.0);
        assert_eq!(live_after, live_before);
        assert!(garbage_after > 0);
    }

    #[test]
    fn rejects_out_of_range_blob_gc_fraction() {
        let dir = tempfile::tempdir().unwrap();
        let config = StorageConfig {
            path: dir.path().to_path_buf(),
            blob_gc_oldest_files_fraction: 1.5,
            ..StorageConfig::default()
        };
        assert!(Storage::open(&config).is_err());
    }
}

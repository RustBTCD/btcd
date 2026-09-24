//! The RocksDB engine.
//!
//! This module is the only place that names RocksDB. Everything above it sees [`Database`].

mod options;

use std::path::Path;

use rocksdb::Cache;
use rocksdb::{
    ColumnFamilyDescriptor, DB, Direction, IteratorMode, Options as RocksDbOptions, WriteBatch,
};

pub use options::RocksOptions;
use options::table_options;

use crate::storage::db::{Batch, Database, Operation, Profile, RawEntries};
use crate::storage::{Error, Result};

pub struct Rocks {
    db: DB,
    tables: Vec<&'static str>,
}

impl Rocks {
    /// Opens the database, creating any table that does not exist yet.
    pub fn open(
        path: impl AsRef<Path>,
        options: &RocksOptions,
        tables: &[(&'static str, Profile)],
    ) -> Result<Self> {
        let cache = Cache::new_lru_cache(options.block_cache_mib * 1024 * 1024);

        let mut db_options = RocksDbOptions::default();
        db_options.create_if_missing(true);
        db_options.create_missing_column_families(true);
        db_options.set_max_open_files(options.max_open_files);
        db_options.set_keep_log_file_num(options.keep_log_files);
        let threads = match options.background_threads {
            0 => std::thread::available_parallelism().map_or(2, |n| n.get()),
            n => n,
        };
        db_options.increase_parallelism(threads as i32);

        let descriptors = tables.iter().map(|(name, profile)| {
            ColumnFamilyDescriptor::new(*name, table_options(&cache, *profile, options))
        });

        let db = DB::open_cf_descriptors(&db_options, path, descriptors).map_err(engine_error)?;
        let hosted = tables.iter().map(|(name, _)| *name).collect();
        Ok(Self { db, tables: hosted })
    }

    fn table(&self, name: &'static str) -> Result<&rocksdb::ColumnFamily> {
        self.db
            .cf_handle(name)
            .ok_or_else(|| Error::Engine(format!("table `{name}` is missing")))
    }
}

impl Database for Rocks {
    fn hosts(&self, table: &'static str) -> bool {
        self.tables.contains(&table)
    }

    fn get_raw(&self, table: &'static str, key: &[u8]) -> Result<Option<Vec<u8>>> {
        let table = self.table(table)?;
        self.db.get_cf(table, key).map_err(engine_error)
    }

    fn get_many_raw(&self, table: &'static str, keys: &[Vec<u8>]) -> Result<Vec<Option<Vec<u8>>>> {
        let handle = self.table(table)?;
        self.db
            .batched_multi_get_cf(handle, keys.iter(), false)
            .into_iter()
            .map(|found| {
                found
                    .map(|value| value.map(|bytes| bytes.to_vec()))
                    .map_err(engine_error)
            })
            .collect()
    }

    fn exists_raw(&self, table: &'static str, key: &[u8]) -> Result<bool> {
        let table = self.table(table)?;
        Ok(self
            .db
            .get_pinned_cf(table, key)
            .map_err(engine_error)?
            .is_some())
    }

    fn scan_raw<'a>(&'a self, table: &'static str, prefix: &[u8]) -> RawEntries<'a> {
        let handle = match self.table(table) {
            Ok(handle) => handle,
            Err(err) => return Box::new(std::iter::once(Err(err))),
        };
        let mode = IteratorMode::From(prefix, Direction::Forward);
        let prefix = prefix.to_vec();
        let entries = self.db.iterator_cf(handle, mode);
        Box::new(
            entries
                .map(|entry| entry.map_err(engine_error))
                .take_while(move |entry| match entry {
                    Ok((key, _)) => key.starts_with(&prefix),
                    Err(_) => true,
                })
                .map(|entry| entry.map(|(key, value)| (key.to_vec(), value.to_vec()))),
        )
    }

    fn commit(&self, batch: Batch) -> Result<()> {
        let mut write = WriteBatch::default();
        for operation in batch.operations() {
            match operation {
                Operation::Put { table, key, value } => {
                    write.put_cf(self.table(table)?, key, value);
                }
                Operation::Delete { table, key } => {
                    write.delete_cf(self.table(table)?, key);
                }
            }
        }
        self.db.write(write).map_err(engine_error)?;
        Ok(())
    }

    fn flush(&self) -> Result<()> {
        self.db.flush().map_err(engine_error)?;
        Ok(())
    }
}

fn engine_error(err: rocksdb::Error) -> Error {
    Error::Engine(err.to_string())
}

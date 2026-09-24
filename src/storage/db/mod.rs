//! The database interface.
//!
//! [`Database`] is byte-oriented and object safe, so a handle is an `Arc<dyn Database>` and
//! nothing above this module names an engine. A handle is opened with the tables it hosts,
//! which is how a node can put different tables in different databases.
//!
//! [`Store`] binds one table to one handle. Writes go through a [`Batch`], which the engine
//! applies in one step, so everything sharing a handle can be written atomically.

pub mod table;

pub use table::{Decode, Encode, Profile, Table};

use std::marker::PhantomData;
use std::sync::Arc;

use crate::storage::{Error, Result};

/// Entries a scan yields: the raw key and the raw value.
pub type RawEntries<'a> = Box<dyn Iterator<Item = Result<(Vec<u8>, Vec<u8>)>> + 'a>;

/// Byte-oriented access to one database. Implemented once per engine.
pub trait Database: Send + Sync + 'static {
    /// Whether this database holds the named table. It was opened with a fixed set.
    fn hosts(&self, table: &'static str) -> bool;

    fn get_raw(&self, table: &'static str, key: &[u8]) -> Result<Option<Vec<u8>>>;

    fn get_many_raw(&self, table: &'static str, keys: &[Vec<u8>]) -> Result<Vec<Option<Vec<u8>>>>;

    fn exists_raw(&self, table: &'static str, key: &[u8]) -> Result<bool>;

    /// Every entry whose key starts with `prefix`, in key order.
    fn scan_raw<'a>(&'a self, table: &'static str, prefix: &[u8]) -> RawEntries<'a>;

    /// Applies a batch. Either every operation lands or none does.
    fn commit(&self, batch: Batch) -> Result<()>;

    /// Makes durable whatever the engine still holds in memory.
    fn flush(&self) -> Result<()>;
}

/// A set of writes waiting to be applied together.
#[derive(Debug, Default)]
pub struct Batch {
    operations: Vec<Operation>,
}

#[derive(Debug)]
pub(crate) enum Operation {
    Put {
        table: &'static str,
        key: Vec<u8>,
        value: Vec<u8>,
    },
    Delete {
        table: &'static str,
        key: Vec<u8>,
    },
}

impl Batch {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.operations.len()
    }

    pub fn is_empty(&self) -> bool {
        self.operations.is_empty()
    }

    pub fn put<T: Table>(&mut self, key: &T::Key, value: &T::Value) -> Result<()> {
        self.operations.push(Operation::Put {
            table: T::NAME,
            key: key.encode(),
            value: value.encode(),
        });
        Ok(())
    }

    pub fn delete<T: Table>(&mut self, key: &T::Key) -> Result<()> {
        self.operations.push(Operation::Delete {
            table: T::NAME,
            key: key.encode(),
        });
        Ok(())
    }

    pub(crate) fn operations(&self) -> &[Operation] {
        &self.operations
    }
}

/// One table on one database.
///
/// Opening a store checks that the database actually hosts the table, so a mistake in how the
/// node is wired together fails at startup rather than on the first write.
pub struct Store<T: Table> {
    db: Arc<dyn Database>,
    table: PhantomData<T>,
}

impl<T: Table> Clone for Store<T> {
    fn clone(&self) -> Self {
        Self {
            db: self.db.clone(),
            table: PhantomData,
        }
    }
}

impl<T: Table> Store<T> {
    pub fn open(db: Arc<dyn Database>) -> Result<Self> {
        if !db.hosts(T::NAME) {
            return Err(Error::Engine(format!(
                "this database does not host the `{}` table",
                T::NAME
            )));
        }
        Ok(Self {
            db,
            table: PhantomData,
        })
    }

    /// The database behind this store, for writing several tables together.
    pub fn database(&self) -> &Arc<dyn Database> {
        &self.db
    }

    pub fn get(&self, key: &T::Key) -> Result<Option<T::Value>> {
        match self.db.get_raw(T::NAME, &key.encode())? {
            Some(bytes) => Ok(Some(T::Value::decode(&bytes)?)),
            None => Ok(None),
        }
    }

    /// One call for many keys, answered in the same order.
    pub fn get_many(&self, keys: &[T::Key]) -> Result<Vec<Option<T::Value>>> {
        let encoded: Vec<Vec<u8>> = keys.iter().map(Encode::encode).collect();
        self.db
            .get_many_raw(T::NAME, &encoded)?
            .into_iter()
            .map(|found| found.map(|bytes| T::Value::decode(&bytes)).transpose())
            .collect()
    }

    pub fn exists(&self, key: &T::Key) -> Result<bool> {
        self.db.exists_raw(T::NAME, &key.encode())
    }

    /// Whether any key starts with `prefix`.
    pub fn exists_prefix(&self, prefix: &T::Prefix) -> Result<bool> {
        match self.db.scan_raw(T::NAME, &prefix.encode()).next() {
            None => Ok(false),
            Some(Err(err)) => Err(err),
            Some(Ok(_)) => Ok(true),
        }
    }

    /// Every value whose key starts with `prefix`, in key order. An empty prefix, which is
    /// what `()` encodes to, walks the whole table.
    pub fn scan<'a>(
        &'a self,
        prefix: &T::Prefix,
    ) -> Box<dyn Iterator<Item = Result<T::Value>> + 'a> {
        let entries = self.db.scan_raw(T::NAME, &prefix.encode());
        Box::new(entries.map(|entry| T::Value::decode(&entry?.1)))
    }

    /// Writes one entry as its own commit.
    pub fn put(&self, key: &T::Key, value: &T::Value) -> Result<()> {
        self.write(|batch| batch.put::<T>(key, value))
    }

    /// Deletes one entry as its own commit.
    pub fn delete(&self, key: &T::Key) -> Result<()> {
        self.write(|batch| batch.delete::<T>(key))
    }

    /// Several writes to this table, applied together.
    pub fn write<R>(&self, changes: impl FnOnce(&mut Batch) -> Result<R>) -> Result<R> {
        let mut batch = Batch::new();
        let result = changes(&mut batch)?;
        self.db.commit(batch)?;
        Ok(result)
    }
}

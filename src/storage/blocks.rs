use std::sync::Arc;

use bitcoin::encoding::{decode_from_slice, encode_to_vec};
use bitcoin::hashes::Hash;
use bitcoin::{Block, BlockHash};
use rocksdb::DB;

use super::{Result, StorageError, cf};

/// Raw block bodies keyed by block hash.
///
/// Blocks are stored as soon as they are received, independent of whether they are on the
/// active chain. Use [`super::ChainStore::block_hash_at`] to find a block by height.
#[derive(Clone)]
pub struct BlockStore {
    db: Arc<DB>,
}

impl BlockStore {
    pub(crate) fn new(db: Arc<DB>) -> Self {
        Self { db }
    }

    pub fn put(&self, block: &Block) -> Result<BlockHash> {
        let hash = block.block_hash();
        let cf = cf(&self.db, cf::BLOCKS)?;
        self.db
            .put_cf(cf, hash.as_byte_array(), encode_to_vec(block))?;
        Ok(hash)
    }

    pub fn get(&self, hash: &BlockHash) -> Result<Option<Block>> {
        let cf = cf(&self.db, cf::BLOCKS)?;
        self.db
            .get_pinned_cf(cf, hash.as_byte_array())?
            .map(|bytes| decode_from_slice(&bytes).map_err(|e| StorageError::corrupted("block", e)))
            .transpose()
    }

    pub fn contains(&self, hash: &BlockHash) -> Result<bool> {
        let cf = cf(&self.db, cf::BLOCKS)?;
        Ok(self.db.get_pinned_cf(cf, hash.as_byte_array())?.is_some())
    }

    pub fn delete(&self, hash: &BlockHash) -> Result<()> {
        let cf = cf(&self.db, cf::BLOCKS)?;
        self.db.delete_cf(cf, hash.as_byte_array())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::storage::test_utils::*;

    #[test]
    fn put_get_delete() {
        let (_dir, storage) = open_temp();
        let blocks = storage.blocks();
        let block = block(null_hash(), 7, vec![coinbase(1, vec![output(50)])]);

        let hash = blocks.put(&block).unwrap();
        assert!(blocks.contains(&hash).unwrap());
        assert_eq!(blocks.get(&hash).unwrap(), Some(block));

        blocks.delete(&hash).unwrap();
        assert!(!blocks.contains(&hash).unwrap());
        assert_eq!(blocks.get(&hash).unwrap(), None);
    }
}

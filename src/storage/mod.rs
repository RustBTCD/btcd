//! Persistent node storage.
//!
//! Tables are declared in [`tables`], engines live in [`engine`], and the interface between
//! them is [`db`]. Which table lives in which database is decided here, in the wiring: the
//! chainstate tables share one database because a block moves them together, while block
//! bodies and headers each get their own.

// Parts of this interface are used by the chain layer, which is the next piece of work.
#![allow(dead_code, unused_imports)]

mod chain;
mod coin;
mod config;
pub mod db;
pub mod engine;
mod error;
mod headers;
pub mod tables;

use std::sync::Arc;

pub use chain::{ChainWrite, Chainstate, InChainstate};
pub use coin::{BlockUndo, Coin};
pub use config::StorageConfig;
pub use db::{Batch, Database, Store, Table};
pub use engine::{Rocks, RocksOptions};
pub use error::{Error, Result};
pub use headers::{BlockStatus, HeaderEntry};
pub use tables::{Blocks, Headers, MetaKey};

/// The node's storage: one database for the chainstate, one for block bodies, one for headers.
#[derive(Clone)]
pub struct Storage {
    chainstate: Chainstate,
    blocks: Store<Blocks>,
    headers: Store<Headers>,
}

impl Storage {
    pub fn open(config: &StorageConfig) -> Result<Self> {
        let chain_db = Rocks::open(
            config.path.join("chainstate"),
            &config.rocks,
            Chainstate::TABLES,
        )?;
        let blocks_db = Rocks::open(
            config.path.join("blocks"),
            &config.rocks,
            &[(Blocks::NAME, Blocks::PROFILE)],
        )?;
        let headers_db = Rocks::open(
            config.path.join("headers"),
            &config.rocks,
            &[(Headers::NAME, Headers::PROFILE)],
        )?;

        Ok(Self {
            chainstate: Chainstate::open(Arc::new(chain_db))?,
            blocks: Store::open(Arc::new(blocks_db))?,
            headers: Store::open(Arc::new(headers_db))?,
        })
    }

    /// Coins, undo data, the height index and the tip, which move together.
    pub fn chainstate(&self) -> &Chainstate {
        &self.chainstate
    }

    /// Block bodies, on any branch.
    pub fn blocks(&self) -> &Store<Blocks> {
        &self.blocks
    }

    /// Every known header.
    pub fn headers(&self) -> &Store<Headers> {
        &self.headers
    }
}

#[cfg(test)]
pub(crate) mod test_utils {
    use std::path::Path;

    use super::{Storage, StorageConfig};
    use bitcoin::block::{Checked, Header, Version as BlockVersion};
    use bitcoin::hashes::Hash;
    use bitcoin::transaction::Version as TxVersion;
    use bitcoin::{
        Amount, Block, BlockHash, BlockTime, CompactTarget, OutPoint, ScriptPubKeyBuf,
        ScriptSigBuf, Sequence, Transaction, TxIn, TxMerkleNode, TxOut, Witness,
        absolute::LockTime,
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
            amount: Amount::from_sat(sats).expect("amount is in range"),
            script_pubkey: ScriptPubKeyBuf::from_bytes(vec![0x51]),
        }
    }

    /// `tag` makes otherwise identical coinbases have different txids.
    pub fn coinbase(tag: u8, outputs: Vec<TxOut>) -> Transaction {
        tx(vec![(OutPoint::COINBASE_PREVOUT, vec![tag, tag])], outputs)
    }

    pub fn spend(prevouts: &[OutPoint], outputs: Vec<TxOut>) -> Transaction {
        tx(prevouts.iter().map(|p| (*p, vec![])).collect(), outputs)
    }

    fn tx(inputs: Vec<(OutPoint, Vec<u8>)>, outputs: Vec<TxOut>) -> Transaction {
        Transaction {
            version: TxVersion::TWO,
            lock_time: LockTime::ZERO,
            inputs: inputs
                .into_iter()
                .map(|(previous_output, sig)| TxIn {
                    previous_output,
                    script_sig: ScriptSigBuf::from_bytes(sig),
                    sequence: Sequence::MAX,
                    witness: Witness::new(),
                })
                .collect(),
            outputs,
        }
    }

    pub fn block(prev: BlockHash, nonce: u32, transactions: Vec<Transaction>) -> Block {
        let header = Header {
            version: BlockVersion::ONE,
            prev_blockhash: prev,
            merkle_root: TxMerkleNode::from_byte_array([0; 32]),
            time: BlockTime::from_u32(0),
            bits: CompactTarget::from_consensus(0x207f_ffff),
            nonce,
        };
        Block::new_unchecked(header, transactions)
    }

    /// Blocks reach the chainstate only after validation, which is what the type says.
    pub fn checked(block: &Block) -> Block<Checked> {
        block.clone().assume_checked(None)
    }

    pub fn null_hash() -> BlockHash {
        BlockHash::from_byte_array([0; 32])
    }
}

#[cfg(test)]
mod tests {
    use super::test_utils::*;
    use super::*;
    use bitcoin::ext::*;
    use bitcoin::pow::Work;

    fn entry(nonce: u32, height: u32) -> HeaderEntry {
        let header = *checked(&block(null_hash(), nonce, vec![])).header();
        let mut status = BlockStatus::HEADER_VALID;
        status.insert(BlockStatus::HAVE_DATA);
        HeaderEntry {
            chain_work: header.work(),
            header,
            height,
            status,
        }
    }

    #[test]
    fn blocks_are_stored_and_deleted() {
        let (_dir, storage) = open_temp();
        let blocks = storage.blocks();
        let block = block(null_hash(), 7, vec![coinbase(1, vec![output(50)])]);
        let hash = block.block_hash();

        blocks.put(&hash, &block).unwrap();
        assert!(blocks.exists(&hash).unwrap());
        assert_eq!(blocks.get(&hash).unwrap(), Some(block));

        blocks.delete(&hash).unwrap();
        assert!(!blocks.exists(&hash).unwrap());
    }

    #[test]
    fn headers_are_written_together_and_read_back() {
        let (_dir, storage) = open_temp();
        let headers = storage.headers();
        let a = entry(1, 10);
        let b = entry(2, 11);

        headers
            .write(|batch| {
                batch.put::<Headers>(&a.block_hash(), &a)?;
                batch.put::<Headers>(&b.block_hash(), &b)
            })
            .unwrap();

        assert_eq!(headers.get(&a.block_hash()).unwrap(), Some(a.clone()));
        let mut all: Vec<HeaderEntry> = headers.scan(&()).collect::<Result<_>>().unwrap();
        all.sort_by_key(|entry| entry.height);
        assert_eq!(all, vec![a, b]);
    }

    #[test]
    fn a_store_refuses_a_database_without_its_table() {
        let dir = tempfile::tempdir().unwrap();
        let options = RocksOptions::default();
        let db = Rocks::open(dir.path(), &options, &[(Blocks::NAME, Blocks::PROFILE)]).unwrap();

        let wrong: Result<Store<Headers>> = Store::open(Arc::new(db));
        assert!(wrong.is_err(), "the headers table is not in this database");
    }

    #[test]
    fn each_group_gets_its_own_database() {
        let (dir, _storage) = open_temp();
        for name in ["chainstate", "blocks", "headers"] {
            assert!(
                dir.path().join(name).is_dir(),
                "{name} has its own database"
            );
        }
    }
}

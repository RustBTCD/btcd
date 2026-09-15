use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use bitcoin::hashes::Hash;
use bitcoin::{Block, BlockHash, OutPoint, TxIn};
use rocksdb::{DB, WriteBatch};

use super::coin::{coin_bytes, undo_bytes};
use super::utxo::outpoint_key;
use super::{BlockUndo, Coin, Result, StorageError, cf, meta_key};

/// The active chain: tip, height index, undo data, and the only writer of the UTXO set.
///
/// Writes are serialized by a mutex shared by every `ChainStore` from the same
/// [`super::Storage`], so the tip check and the write inside `connect_block` and
/// `disconnect_block` cannot interleave. Reads take no lock.
///
/// Future improvement: replace the mutex with a dedicated writer thread fed by a bounded queue.
/// Callers would stop blocking for a whole block connect, and the queue would give
/// backpressure during initial block download.
#[derive(Clone)]
pub struct ChainStore {
    db: Arc<DB>,
    write_lock: Arc<Mutex<()>>,
}

impl ChainStore {
    pub(crate) fn new(db: Arc<DB>, write_lock: Arc<Mutex<()>>) -> Self {
        Self { db, write_lock }
    }

    /// A panic while holding the lock cannot leave a half-applied write, because every write
    /// is one atomic RocksDB batch. Poisoning is therefore safe to ignore.
    fn lock_writes(&self) -> MutexGuard<'_, ()> {
        self.write_lock
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    pub fn tip(&self) -> Result<Option<BlockHash>> {
        let cf = cf(&self.db, cf::META)?;
        self.db
            .get_pinned_cf(cf, meta_key::TIP)?
            .map(|b| decode_hash(&b))
            .transpose()
    }

    /// Hash of the active-chain block at `height`.
    pub fn block_hash_at(&self, height: u32) -> Result<Option<BlockHash>> {
        let cf = cf(&self.db, cf::HEIGHT_INDEX)?;
        self.db
            .get_pinned_cf(cf, height.to_be_bytes())?
            .map(|b| decode_hash(&b))
            .transpose()
    }

    pub fn undo(&self, hash: &BlockHash) -> Result<Option<BlockUndo>> {
        let cf = cf(&self.db, cf::UNDO)?;
        self.db
            .get_pinned_cf(cf, hash.as_byte_array())?
            .map(|b| BlockUndo::from_bytes(&b))
            .transpose()
    }

    /// Applies a validated block on top of the current tip in one atomic write.
    ///
    /// `spent` holds the coins consumed by the block, one per non-coinbase input, in block
    /// order. They become the block's undo data.
    ///
    /// Within the batch, new outputs are written before spent ones are deleted, so an output
    /// created and spent inside the same block ends up absent from the UTXO set.
    pub fn connect_block(&self, block: &Block, height: u32, spent: &[Coin]) -> Result<()> {
        let _guard = self.lock_writes();
        let hash = block.block_hash();
        let tip = self.tip()?;
        if tip.unwrap_or_else(BlockHash::all_zeros) != block.header.prev_blockhash {
            return Err(StorageError::NotExtendingTip { block: hash, tip });
        }
        check_spent_count(block, spent.len())?;

        let utxo = cf(&self.db, cf::UTXO)?;
        let mut batch = WriteBatch::default();

        for tx in &block.txdata {
            let txid = tx.compute_txid();
            let is_coinbase = tx.is_coinbase();
            for (vout, output) in tx.output.iter().enumerate() {
                if Coin::is_unspendable(output) {
                    continue;
                }
                let key = outpoint_key(&OutPoint {
                    txid,
                    vout: vout as u32,
                });
                batch.put_cf(utxo, key, coin_bytes(output, height, is_coinbase));
            }
        }
        for input in spending_inputs(block) {
            batch.delete_cf(utxo, outpoint_key(&input.previous_output));
        }

        batch.put_cf(
            cf(&self.db, cf::UNDO)?,
            hash.as_byte_array(),
            undo_bytes(spent),
        );
        batch.put_cf(
            cf(&self.db, cf::HEIGHT_INDEX)?,
            height.to_be_bytes(),
            hash.as_byte_array(),
        );
        batch.put_cf(cf(&self.db, cf::META)?, meta_key::TIP, hash.as_byte_array());

        self.db.write(batch)?;
        Ok(())
    }

    /// Reverts the current tip block in one atomic write, restoring the coins it spent.
    ///
    /// Spent coins are restored before created outputs are deleted, so an output created and
    /// spent inside the same block ends up absent from the UTXO set.
    pub fn disconnect_block(&self, block: &Block, height: u32) -> Result<()> {
        let _guard = self.lock_writes();
        let hash = block.block_hash();
        let tip = self.tip()?;
        if tip != Some(hash) {
            return Err(StorageError::NotTip { block: hash, tip });
        }
        let undo = self.undo(&hash)?.ok_or(StorageError::MissingUndo(hash))?;
        check_spent_count(block, undo.0.len())?;

        let utxo = cf(&self.db, cf::UTXO)?;
        let mut batch = WriteBatch::default();

        for (input, coin) in spending_inputs(block).zip(&undo.0) {
            let key = outpoint_key(&input.previous_output);
            batch.put_cf(
                utxo,
                key,
                coin_bytes(&coin.output, coin.height, coin.is_coinbase),
            );
        }
        for tx in &block.txdata {
            let txid = tx.compute_txid();
            // Unspendable outputs were never inserted; deleting a missing key is a no-op.
            for vout in 0..tx.output.len() {
                batch.delete_cf(
                    utxo,
                    outpoint_key(&OutPoint {
                        txid,
                        vout: vout as u32,
                    }),
                );
            }
        }

        batch.delete_cf(cf(&self.db, cf::UNDO)?, hash.as_byte_array());
        batch.delete_cf(cf(&self.db, cf::HEIGHT_INDEX)?, height.to_be_bytes());
        batch.put_cf(
            cf(&self.db, cf::META)?,
            meta_key::TIP,
            block.header.prev_blockhash.as_byte_array(),
        );

        self.db.write(batch)?;
        Ok(())
    }
}

/// Inputs that spend existing coins: every input except the coinbase's.
fn spending_inputs(block: &Block) -> impl Iterator<Item = &TxIn> {
    block.txdata.iter().skip(1).flat_map(|tx| &tx.input)
}

fn check_spent_count(block: &Block, actual: usize) -> Result<()> {
    let expected = spending_inputs(block).count();
    if expected == actual {
        Ok(())
    } else {
        Err(StorageError::SpentCoinsMismatch { expected, actual })
    }
}

fn decode_hash(bytes: &[u8]) -> Result<BlockHash> {
    let array: [u8; 32] = bytes
        .try_into()
        .map_err(|_| StorageError::corrupted("block hash", "expected 32 bytes"))?;
    Ok(BlockHash::from_byte_array(array))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::test_utils::*;
    use bitcoin::{Amount, ScriptBuf, TxOut};

    fn op(tx: &bitcoin::Transaction, vout: u32) -> OutPoint {
        OutPoint {
            txid: tx.compute_txid(),
            vout,
        }
    }

    #[test]
    fn connect_and_disconnect_restore_state() {
        let (_dir, storage) = open_temp();
        let chain = storage.chain();
        let utxos = storage.utxos();

        // Block 0: coinbase with two outputs and one OP_RETURN.
        let op_return = TxOut {
            value: Amount::ZERO,
            script_pubkey: ScriptBuf::from_bytes(vec![0x6a]),
        };
        let cb0 = coinbase(0, vec![output(50), output(25), op_return]);
        let b0 = block(null_hash(), 0, vec![cb0.clone()]);
        chain.connect_block(&b0, 0, &[]).unwrap();

        assert_eq!(chain.tip().unwrap(), Some(b0.block_hash()));
        assert_eq!(chain.block_hash_at(0).unwrap(), Some(b0.block_hash()));
        let coin0 = utxos.get(&op(&cb0, 0)).unwrap().unwrap();
        assert_eq!(
            coin0,
            Coin {
                output: output(50),
                height: 0,
                is_coinbase: true
            }
        );
        assert!(
            utxos.get(&op(&cb0, 2)).unwrap().is_none(),
            "OP_RETURN must not be stored"
        );

        // Block 1: spends cb0:0, and tx_b spends tx_a's output inside the same block.
        let cb1 = coinbase(1, vec![output(50)]);
        let tx_a = spend(&[op(&cb0, 0)], vec![output(40)]);
        let tx_b = spend(&[op(&tx_a, 0)], vec![output(30)]);
        let b1 = block(
            b0.block_hash(),
            1,
            vec![cb1.clone(), tx_a.clone(), tx_b.clone()],
        );
        let spent = vec![
            coin0.clone(),
            Coin {
                output: output(40),
                height: 1,
                is_coinbase: false,
            },
        ];
        chain.connect_block(&b1, 1, &spent).unwrap();

        assert_eq!(chain.tip().unwrap(), Some(b1.block_hash()));
        assert!(utxos.get(&op(&cb0, 0)).unwrap().is_none());
        assert!(
            utxos.get(&op(&tx_a, 0)).unwrap().is_none(),
            "intra-block spend"
        );
        assert!(utxos.get(&op(&tx_b, 0)).unwrap().is_some());
        assert!(utxos.get(&op(&cb1, 0)).unwrap().is_some());
        assert_eq!(
            chain.undo(&b1.block_hash()).unwrap(),
            Some(BlockUndo(spent))
        );

        // Disconnect block 1: state equals the state after block 0.
        chain.disconnect_block(&b1, 1).unwrap();

        assert_eq!(chain.tip().unwrap(), Some(b0.block_hash()));
        assert_eq!(chain.block_hash_at(1).unwrap(), None);
        assert_eq!(chain.undo(&b1.block_hash()).unwrap(), None);
        assert_eq!(utxos.get(&op(&cb0, 0)).unwrap(), Some(coin0));
        assert!(utxos.get(&op(&cb0, 1)).unwrap().is_some());
        assert!(utxos.get(&op(&tx_a, 0)).unwrap().is_none());
        assert!(utxos.get(&op(&tx_b, 0)).unwrap().is_none());
        assert!(utxos.get(&op(&cb1, 0)).unwrap().is_none());
    }

    #[test]
    fn get_many_and_prefix_lookup() {
        let (_dir, storage) = open_temp();
        let cb = coinbase(0, vec![output(1), output(2), output(3)]);
        let b0 = block(null_hash(), 0, vec![cb.clone()]);
        storage.chain().connect_block(&b0, 0, &[]).unwrap();

        let utxos = storage.utxos();
        let missing = OutPoint {
            txid: cb.compute_txid(),
            vout: 9,
        };
        let found = utxos.get_many(&[op(&cb, 2), missing, op(&cb, 0)]).unwrap();
        assert_eq!(
            found[0].as_ref().map(|c| c.output.value),
            Some(Amount::from_sat(3))
        );
        assert!(found[1].is_none());
        assert_eq!(
            found[2].as_ref().map(|c| c.output.value),
            Some(Amount::from_sat(1))
        );

        assert!(utxos.has_unspent_outputs(&cb.compute_txid()).unwrap());
        let other = coinbase(7, vec![output(1)]).compute_txid();
        assert!(!utxos.has_unspent_outputs(&other).unwrap());
    }

    #[test]
    fn rejects_blocks_not_on_tip() {
        let (_dir, storage) = open_temp();
        let chain = storage.chain();
        let b0 = block(null_hash(), 0, vec![coinbase(0, vec![output(1)])]);
        let orphan = block(b0.block_hash(), 5, vec![coinbase(5, vec![output(1)])]);

        assert!(matches!(
            chain.connect_block(&orphan, 1, &[]),
            Err(StorageError::NotExtendingTip { .. })
        ));
        chain.connect_block(&b0, 0, &[]).unwrap();
        assert!(matches!(
            chain.disconnect_block(&orphan, 1),
            Err(StorageError::NotTip { .. })
        ));
    }

    #[test]
    fn rejects_wrong_spent_count() {
        let (_dir, storage) = open_temp();
        let cb = coinbase(0, vec![output(1)]);
        let b0 = block(
            null_hash(),
            0,
            vec![cb.clone(), spend(&[op(&cb, 0)], vec![output(1)])],
        );
        assert!(matches!(
            storage.chain().connect_block(&b0, 0, &[]),
            Err(StorageError::SpentCoinsMismatch {
                expected: 1,
                actual: 0
            })
        ));
    }

    #[test]
    fn state_survives_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let cb = coinbase(0, vec![output(50)]);
        let b0 = block(null_hash(), 0, vec![cb.clone()]);
        {
            let storage = open_at(dir.path());
            storage.chain().connect_block(&b0, 0, &[]).unwrap();
        }
        let storage = open_at(dir.path());
        assert_eq!(storage.chain().tip().unwrap(), Some(b0.block_hash()));
        assert!(storage.utxos().get(&op(&cb, 0)).unwrap().is_some());
    }

    #[test]
    fn concurrent_connects_on_same_tip_admit_one() {
        let (_dir, storage) = open_temp();
        let b0 = block(null_hash(), 0, vec![coinbase(0, vec![output(1)])]);
        storage.chain().connect_block(&b0, 0, &[]).unwrap();

        const THREADS: u8 = 8;
        let barrier = std::sync::Barrier::new(usize::from(THREADS));
        let successes = std::thread::scope(|s| {
            let handles: Vec<_> = (1..=THREADS)
                .map(|i| {
                    let chain = storage.chain();
                    let barrier = &barrier;
                    let b1 = block(
                        b0.block_hash(),
                        u32::from(i),
                        vec![coinbase(i, vec![output(1)])],
                    );
                    s.spawn(move || {
                        barrier.wait();
                        chain.connect_block(&b1, 1, &[]).is_ok()
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|h| h.join().unwrap())
                .filter(|ok| *ok)
                .count()
        });

        assert_eq!(successes, 1);
        let tip = storage.chain().tip().unwrap().unwrap();
        assert_ne!(tip, b0.block_hash());
        assert_eq!(storage.chain().block_hash_at(1).unwrap(), Some(tip));
    }
}

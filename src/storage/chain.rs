//! The chainstate: the tables that must move together.
//!
//! Coins, undo data, the height index and the tip share one database, because a block changes
//! all four and the coin set means nothing without the tip that explains it. Anything that
//! does not need that, such as block bodies, lives elsewhere.

use std::sync::Arc;

use bitcoin::block::Checked;
use bitcoin::hashes::Hash;
use bitcoin::{Block, BlockHash, OutPoint, TxIn};

use super::db::{Batch, Database, Store, Table};
use super::tables::{Coins, HeightIndex, Meta, MetaKey, Undo};
use super::{BlockUndo, Coin, Error, Result};

/// The tables that belong to the chainstate. Only these can be written in one of its
/// transactions, which is checked when this code is compiled.
pub trait InChainstate: Table {}

impl InChainstate for Coins {}
impl InChainstate for Undo {}
impl InChainstate for HeightIndex {}
impl InChainstate for Meta {}

/// Tables that share one database, and the writes that move them together.
///
/// Assumes a single writer. The tip check and the write are not atomic against concurrent
/// callers.
#[derive(Clone)]
pub struct Chainstate {
    db: Arc<dyn Database>,
    coins: Store<Coins>,
    undo: Store<Undo>,
    heights: Store<HeightIndex>,
    meta: Store<Meta>,
}

impl Chainstate {
    /// Every table must live in the given database, which is checked here so a wiring mistake
    /// fails at startup.
    pub fn open(db: Arc<dyn Database>) -> Result<Self> {
        Ok(Self {
            coins: Store::open(db.clone())?,
            undo: Store::open(db.clone())?,
            heights: Store::open(db.clone())?,
            meta: Store::open(db.clone())?,
            db,
        })
    }

    /// The tables the node needs to declare when opening the database.
    pub const TABLES: &'static [(&'static str, super::db::Profile)] = &[
        (Coins::NAME, Coins::PROFILE),
        (Undo::NAME, Undo::PROFILE),
        (HeightIndex::NAME, HeightIndex::PROFILE),
        (Meta::NAME, Meta::PROFILE),
    ];

    /// Read access to the unspent output set. Writes go through [`Chainstate::write`].
    pub fn coins(&self) -> &Store<Coins> {
        &self.coins
    }

    /// The block whose state the unspent output set represents.
    pub fn tip(&self) -> Result<Option<BlockHash>> {
        self.meta.get(&MetaKey::Tip)
    }

    /// Hash of the active chain's block at `height`.
    pub fn block_hash_at(&self, height: u32) -> Result<Option<BlockHash>> {
        self.heights.get(&height)
    }

    pub fn undo(&self, hash: &BlockHash) -> Result<Option<BlockUndo>> {
        self.undo.get(hash)
    }

    /// Whether any output of `txid` is still unspent.
    pub fn has_unspent_outputs(&self, txid: &bitcoin::Txid) -> Result<bool> {
        self.coins.exists_prefix(txid)
    }

    /// Writes several tables of the chainstate in one step. Returning an error writes nothing.
    pub fn write<R>(&self, changes: impl FnOnce(&mut ChainWrite) -> Result<R>) -> Result<R> {
        let mut write = ChainWrite {
            batch: Batch::new(),
        };
        let result = changes(&mut write)?;
        self.db.commit(write.batch)?;
        Ok(result)
    }

    /// Applies a validated block on top of the current tip in one write.
    ///
    /// `spent` holds the coins the block consumed, one per non-coinbase input, in block order.
    /// They become the block's undo data.
    ///
    /// New outputs are written before spent ones are deleted, so an output created and spent
    /// inside the same block ends up absent.
    pub fn connect_block(&self, block: &Block<Checked>, height: u32, spent: &[Coin]) -> Result<()> {
        let hash = block.block_hash();
        let tip = self.tip()?;
        if tip.unwrap_or(BlockHash::from_byte_array([0; 32])) != block.header().prev_blockhash {
            return Err(Error::NotExtendingTip { block: hash, tip });
        }
        check_spent_count(block, spent.len())?;

        self.write(|write| {
            for transaction in block.transactions() {
                let txid = transaction.compute_txid();
                let is_coinbase = transaction.is_coinbase();
                for (vout, output) in transaction.outputs.iter().enumerate() {
                    if Coin::is_unspendable(output) {
                        continue;
                    }
                    let outpoint = OutPoint {
                        txid,
                        vout: vout as u32,
                    };
                    let coin = Coin {
                        output: output.clone(),
                        height,
                        is_coinbase,
                    };
                    write.put::<Coins>(&outpoint, &coin)?;
                }
            }
            for input in spending_inputs(block) {
                write.delete::<Coins>(&input.previous_output)?;
            }

            write.put::<Undo>(&hash, &BlockUndo(spent.to_vec()))?;
            write.put::<HeightIndex>(&height, &hash)?;
            write.put::<Meta>(&MetaKey::Tip, &hash)
        })
    }

    /// Reverts the current tip block in one write, restoring the coins it spent.
    ///
    /// Spent coins are restored before created outputs are deleted, so an output created and
    /// spent inside the same block ends up absent.
    pub fn disconnect_block(&self, block: &Block<Checked>, height: u32) -> Result<()> {
        let hash = block.block_hash();
        let tip = self.tip()?;
        if tip != Some(hash) {
            return Err(Error::NotTip { block: hash, tip });
        }
        let undo = self.undo(&hash)?.ok_or(Error::MissingUndo(hash))?;
        check_spent_count(block, undo.0.len())?;

        self.write(|write| {
            for (input, coin) in spending_inputs(block).zip(&undo.0) {
                write.put::<Coins>(&input.previous_output, coin)?;
            }
            for transaction in block.transactions() {
                let txid = transaction.compute_txid();
                // Unspendable outputs were never inserted; deleting a missing key is a no-op.
                for vout in 0..transaction.outputs.len() {
                    write.delete::<Coins>(&OutPoint {
                        txid,
                        vout: vout as u32,
                    })?;
                }
            }

            write.delete::<Undo>(&hash)?;
            write.delete::<HeightIndex>(&height)?;
            write.put::<Meta>(&MetaKey::Tip, &block.header().prev_blockhash)
        })
    }
}

/// Writes collected inside [`Chainstate::write`]. Only chainstate tables are reachable.
pub struct ChainWrite {
    batch: Batch,
}

impl ChainWrite {
    pub fn put<T: InChainstate>(&mut self, key: &T::Key, value: &T::Value) -> Result<()> {
        self.batch.put::<T>(key, value)
    }

    pub fn delete<T: InChainstate>(&mut self, key: &T::Key) -> Result<()> {
        self.batch.delete::<T>(key)
    }
}

/// Inputs that spend existing coins: every input except the coinbase's.
fn spending_inputs(block: &Block<Checked>) -> impl Iterator<Item = &TxIn> {
    block
        .transactions()
        .iter()
        .skip(1)
        .flat_map(|transaction| &transaction.inputs)
}

fn check_spent_count(block: &Block<Checked>, actual: usize) -> Result<()> {
    let expected = spending_inputs(block).count();
    if expected == actual {
        Ok(())
    } else {
        Err(Error::SpentCoinsMismatch { expected, actual })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::test_utils::*;
    use bitcoin::{Amount, ScriptPubKeyBuf, TxOut};

    fn op(transaction: &bitcoin::Transaction, vout: u32) -> OutPoint {
        OutPoint {
            txid: transaction.compute_txid(),
            vout,
        }
    }

    #[test]
    fn connect_and_disconnect_restore_state() {
        let (_dir, storage) = open_temp();
        let chain = storage.chainstate();

        // Block 0: a coinbase with two spendable outputs and one that can never be spent.
        let op_return = TxOut {
            amount: Amount::ZERO,
            script_pubkey: ScriptPubKeyBuf::from_bytes(vec![0x6a]),
        };
        let cb0 = coinbase(0, vec![output(50), output(25), op_return]);
        let b0 = block(null_hash(), 0, vec![cb0.clone()]);
        chain.connect_block(&checked(&b0), 0, &[]).unwrap();

        assert_eq!(chain.tip().unwrap(), Some(b0.block_hash()));
        assert_eq!(chain.block_hash_at(0).unwrap(), Some(b0.block_hash()));
        let coin0 = chain.coins().get(&op(&cb0, 0)).unwrap().unwrap();
        assert_eq!(
            coin0,
            Coin {
                output: output(50),
                height: 0,
                is_coinbase: true
            }
        );
        assert!(
            chain.coins().get(&op(&cb0, 2)).unwrap().is_none(),
            "an unspendable output is not stored"
        );

        // Block 1 spends one coin, and one of its transactions spends another's output.
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
        chain.connect_block(&checked(&b1), 1, &spent).unwrap();

        assert_eq!(chain.tip().unwrap(), Some(b1.block_hash()));
        assert!(chain.coins().get(&op(&cb0, 0)).unwrap().is_none());
        assert!(
            chain.coins().get(&op(&tx_a, 0)).unwrap().is_none(),
            "created and spent in one block"
        );
        assert!(chain.coins().get(&op(&tx_b, 0)).unwrap().is_some());
        assert_eq!(
            chain.undo(&b1.block_hash()).unwrap(),
            Some(BlockUndo(spent))
        );

        chain.disconnect_block(&checked(&b1), 1).unwrap();

        assert_eq!(chain.tip().unwrap(), Some(b0.block_hash()));
        assert_eq!(chain.block_hash_at(1).unwrap(), None);
        assert_eq!(chain.undo(&b1.block_hash()).unwrap(), None);
        assert_eq!(chain.coins().get(&op(&cb0, 0)).unwrap(), Some(coin0));
        assert!(chain.coins().get(&op(&cb0, 1)).unwrap().is_some());
        assert!(chain.coins().get(&op(&tx_b, 0)).unwrap().is_none());
        assert!(chain.coins().get(&op(&cb1, 0)).unwrap().is_none());
    }

    #[test]
    fn get_many_and_prefix_lookup() {
        let (_dir, storage) = open_temp();
        let chain = storage.chainstate();
        let cb = coinbase(0, vec![output(1), output(2), output(3)]);
        let b0 = block(null_hash(), 0, vec![cb.clone()]);
        chain.connect_block(&checked(&b0), 0, &[]).unwrap();

        let missing = OutPoint {
            txid: cb.compute_txid(),
            vout: 9,
        };
        let found = chain
            .coins()
            .get_many(&[op(&cb, 2), missing, op(&cb, 0)])
            .unwrap();
        assert_eq!(
            found[0].as_ref().map(|c| c.output.amount),
            Some(Amount::from_sat(3).unwrap())
        );
        assert!(found[1].is_none());
        assert_eq!(
            found[2].as_ref().map(|c| c.output.amount),
            Some(Amount::from_sat(1).unwrap())
        );

        assert!(chain.has_unspent_outputs(&cb.compute_txid()).unwrap());
        let other = coinbase(7, vec![output(1)]).compute_txid();
        assert!(!chain.has_unspent_outputs(&other).unwrap());
    }

    #[test]
    fn rejects_blocks_not_on_tip() {
        let (_dir, storage) = open_temp();
        let chain = storage.chainstate();
        let b0 = block(null_hash(), 0, vec![coinbase(0, vec![output(1)])]);
        let orphan = block(b0.block_hash(), 5, vec![coinbase(5, vec![output(1)])]);

        assert!(matches!(
            chain.connect_block(&checked(&orphan), 1, &[]),
            Err(Error::NotExtendingTip { .. })
        ));
        chain.connect_block(&checked(&b0), 0, &[]).unwrap();
        assert!(matches!(
            chain.disconnect_block(&checked(&orphan), 1),
            Err(Error::NotTip { .. })
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
            storage.chainstate().connect_block(&checked(&b0), 0, &[]),
            Err(Error::SpentCoinsMismatch {
                expected: 1,
                actual: 0
            })
        ));
    }

    #[test]
    fn a_failed_write_changes_nothing() {
        let (_dir, storage) = open_temp();
        let chain = storage.chainstate();
        let hash = null_hash();

        let outcome: Result<()> = chain.write(|write| {
            write.put::<Meta>(&MetaKey::Tip, &hash)?;
            Err(Error::MissingUndo(hash))
        });

        assert!(outcome.is_err());
        assert_eq!(
            chain.tip().unwrap(),
            None,
            "nothing is written when the closure fails"
        );
    }

    #[test]
    fn state_survives_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let cb = coinbase(0, vec![output(50)]);
        let b0 = block(null_hash(), 0, vec![cb.clone()]);
        {
            let storage = open_at(dir.path());
            storage
                .chainstate()
                .connect_block(&checked(&b0), 0, &[])
                .unwrap();
        }
        let storage = open_at(dir.path());
        assert_eq!(storage.chainstate().tip().unwrap(), Some(b0.block_hash()));
        assert!(
            storage
                .chainstate()
                .coins()
                .get(&op(&cb, 0))
                .unwrap()
                .is_some()
        );
    }
}

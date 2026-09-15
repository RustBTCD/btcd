use std::sync::Arc;

use bitcoin::hashes::Hash;
use bitcoin::{OutPoint, Txid};
use rocksdb::{DB, Direction, IteratorMode};

use super::{Coin, Result, cf};

pub(crate) const OUTPOINT_KEY_LEN: usize = 36;

/// `txid ‖ vout (u32 BE)`. Big-endian keeps a transaction's outputs adjacent and ordered,
/// so "does this txid have unspent outputs" is a single prefix seek.
pub(crate) fn outpoint_key(outpoint: &OutPoint) -> [u8; OUTPOINT_KEY_LEN] {
    let mut key = [0u8; OUTPOINT_KEY_LEN];
    key[..32].copy_from_slice(outpoint.txid.as_byte_array());
    key[32..].copy_from_slice(&outpoint.vout.to_be_bytes());
    key
}

/// Read access to the UTXO set.
///
/// Writes go only through [`super::ChainStore::connect_block`] and
/// [`super::ChainStore::disconnect_block`], so the set always matches the stored tip.
#[derive(Clone)]
pub struct UtxoStore {
    db: Arc<DB>,
}

impl UtxoStore {
    pub(crate) fn new(db: Arc<DB>) -> Self {
        Self { db }
    }

    pub fn get(&self, outpoint: &OutPoint) -> Result<Option<Coin>> {
        let cf = cf(&self.db, cf::UTXO)?;
        self.db
            .get_pinned_cf(cf, outpoint_key(outpoint))?
            .map(|bytes| Coin::from_bytes(&bytes))
            .transpose()
    }

    /// Looks up many outpoints in one call. Results are in the same order as `outpoints`.
    pub fn get_many(&self, outpoints: &[OutPoint]) -> Result<Vec<Option<Coin>>> {
        let cf = cf(&self.db, cf::UTXO)?;
        let keys: Vec<[u8; OUTPOINT_KEY_LEN]> = outpoints.iter().map(outpoint_key).collect();
        self.db
            .batched_multi_get_cf(cf, keys.iter(), false)
            .into_iter()
            .map(|res| res?.map(|bytes| Coin::from_bytes(&bytes)).transpose())
            .collect()
    }

    /// Whether any output of `txid` is still unspent. Used by the BIP30 check.
    pub fn has_unspent_outputs(&self, txid: &Txid) -> Result<bool> {
        let cf = cf(&self.db, cf::UTXO)?;
        let prefix = txid.as_byte_array();
        let mut iter = self
            .db
            .iterator_cf(cf, IteratorMode::From(prefix, Direction::Forward));
        match iter.next() {
            None => Ok(false),
            Some(Err(e)) => Err(e.into()),
            Some(Ok((key, _))) => Ok(key.starts_with(prefix)),
        }
    }
}

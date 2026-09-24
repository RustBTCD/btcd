//! The tables of the node database, and the codecs their keys use.
//!
//! Every table is declared here. Which database holds which table is decided where the node
//! is wired together, by opening an engine with the tables it should hold.

use bitcoin::{Block, BlockHash, OutPoint, Txid};

use super::db::table::{Decode, Encode};
use super::db::{Profile, Table};
use super::{BlockUndo, Coin, Error, HeaderEntry, Result};

/// Unspent outputs. Keys are an outpoint, and a scan by transaction identifier answers
/// whether a transaction still has unspent outputs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Coins;

impl Table for Coins {
    const NAME: &'static str = "coins";
    const PROFILE: Profile = Profile::PointLookup;

    type Key = OutPoint;
    type Value = Coin;
    type Prefix = Txid;
}

/// The coins each block spent, which is what a disconnect restores.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Undo;

impl Table for Undo {
    const NAME: &'static str = "undo";
    const PROFILE: Profile = Profile::LargeValues;

    type Key = BlockHash;
    type Value = BlockUndo;
    type Prefix = ();
}

/// Block bodies, as received, on any branch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Blocks;

impl Table for Blocks {
    const NAME: &'static str = "blocks";
    const PROFILE: Profile = Profile::LargeValues;

    type Key = BlockHash;
    type Value = Block;
    type Prefix = ();
}

/// Every known header with the metadata derived when it was accepted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Headers;

impl Table for Headers {
    const NAME: &'static str = "headers";
    const PROFILE: Profile = Profile::PointLookup;

    type Key = BlockHash;
    type Value = HeaderEntry;
    type Prefix = ();
}

/// Hash of the active chain's block at a height.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeightIndex;

impl Table for HeightIndex {
    const NAME: &'static str = "height_index";
    const PROFILE: Profile = Profile::Sequential;

    type Key = u32;
    type Value = BlockHash;
    type Prefix = ();
}

/// Single values that describe the database as a whole.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Meta;

impl Table for Meta {
    const NAME: &'static str = "meta";
    const PROFILE: Profile = Profile::Sequential;

    type Key = MetaKey;
    type Value = BlockHash;
    type Prefix = ();
}

/// Keys of the `meta` table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetaKey {
    /// The block whose state the unspent output set represents.
    Tip,
}

impl Encode for MetaKey {
    fn encode(&self) -> Vec<u8> {
        match self {
            MetaKey::Tip => b"tip".to_vec(),
        }
    }
}

/// An outpoint is the transaction identifier followed by the output index, big-endian, so a
/// transaction's outputs sit together and in order.
impl Encode for OutPoint {
    fn encode(&self) -> Vec<u8> {
        let mut key = Vec::with_capacity(36);
        key.extend_from_slice(self.txid.as_byte_array());
        key.extend_from_slice(&self.vout.to_be_bytes());
        key
    }
}

impl Encode for Txid {
    fn encode(&self) -> Vec<u8> {
        self.as_byte_array().to_vec()
    }
}

impl Encode for BlockHash {
    fn encode(&self) -> Vec<u8> {
        self.as_byte_array().to_vec()
    }
}

impl Decode for BlockHash {
    fn decode(bytes: &[u8]) -> Result<Self> {
        let hash: [u8; 32] = bytes
            .try_into()
            .map_err(|_| Error::corrupted("block hash", "expected 32 bytes"))?;
        Ok(BlockHash::from_byte_array(hash))
    }
}

impl Encode for Block {
    fn encode(&self) -> Vec<u8> {
        bitcoin::encoding::encode_to_vec(self)
    }
}

impl Decode for Block {
    fn decode(bytes: &[u8]) -> Result<Self> {
        bitcoin::encoding::decode_from_slice(bytes).map_err(|e| Error::corrupted("block", e))
    }
}

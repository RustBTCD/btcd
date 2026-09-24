use bitcoin::BlockHash;
use bitcoin::block::Header;
use bitcoin::encoding::{decode_from_slice, encode_to_vec};
use bitcoin::pow::Work;

use super::db::table::{Decode, Encode};
use super::{Error, Result};

/// Validation progress of a block, as bit flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BlockStatus(u8);

impl BlockStatus {
    /// Header passed context-free and contextual header checks.
    pub const HEADER_VALID: Self = Self(1 << 0);
    /// Block body is present in the block store.
    pub const HAVE_DATA: Self = Self(1 << 1);
    /// Block was fully validated and connected at least once.
    pub const BLOCK_VALID: Self = Self(1 << 2);
    /// Block or one of its ancestors failed validation.
    pub const FAILED: Self = Self(1 << 3);

    pub const fn bits(self) -> u8 {
        self.0
    }

    pub const fn from_bits(bits: u8) -> Self {
        Self(bits)
    }

    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    pub fn insert(&mut self, other: Self) {
        self.0 |= other.0;
    }

    pub fn remove(&mut self, other: Self) {
        self.0 &= !other.0;
    }
}

const HEADER_LEN: usize = 80;
const HEIGHT_END: usize = HEADER_LEN + 4;
const WORK_END: usize = HEIGHT_END + 32;
/// Stored size of a [`HeaderEntry`]: header, height, chain work, status.
const ENTRY_LEN: usize = WORK_END + 1;

/// A header plus the chain metadata derived when it was accepted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeaderEntry {
    pub header: Header,
    pub height: u32,
    /// Cumulative proof of work from genesis up to and including this block.
    pub chain_work: Work,
    pub status: BlockStatus,
}

impl HeaderEntry {
    pub fn block_hash(&self) -> BlockHash {
        self.header.block_hash()
    }

    /// Layout: header (80 bytes) ‖ height (u32 LE) ‖ chain work (32 bytes BE) ‖ status (u8).
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(ENTRY_LEN);
        bytes.extend_from_slice(&encode_to_vec(&self.header));
        bytes.extend_from_slice(&self.height.to_le_bytes());
        bytes.extend_from_slice(&self.chain_work.to_be_bytes());
        bytes.push(self.status.bits());
        bytes
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() != ENTRY_LEN {
            return Err(Error::corrupted(
                "header entry",
                format!("expected {ENTRY_LEN} bytes, got {}", bytes.len()),
            ));
        }
        let header = decode_from_slice::<Header>(&bytes[..HEADER_LEN])
            .map_err(|e| Error::corrupted("header entry", e))?;
        let height = &bytes[HEADER_LEN..HEIGHT_END];
        let chain_work = &bytes[HEIGHT_END..WORK_END];
        Ok(HeaderEntry {
            header,
            height: u32::from_le_bytes(height.try_into().expect("slice is 4 bytes")),
            chain_work: Work::from_be_bytes(chain_work.try_into().expect("slice is 32 bytes")),
            status: BlockStatus::from_bits(bytes[WORK_END]),
        })
    }
}

impl Encode for HeaderEntry {
    fn encode(&self) -> Vec<u8> {
        self.to_bytes()
    }
}

impl Decode for HeaderEntry {
    fn decode(bytes: &[u8]) -> Result<Self> {
        HeaderEntry::from_bytes(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bitcoin::ext::*;

    use crate::storage::test_utils::*;

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
    fn entry_roundtrip() {
        let e = entry(3, 840_000);
        let bytes = e.to_bytes();
        assert_eq!(bytes.len(), ENTRY_LEN);
        assert_eq!(HeaderEntry::from_bytes(&bytes).unwrap(), e);
    }

    #[test]
    fn entry_rejects_wrong_length() {
        let bytes = entry(3, 1).to_bytes();
        assert!(HeaderEntry::from_bytes(&bytes[..ENTRY_LEN - 1]).is_err());
        assert!(HeaderEntry::from_bytes(&[bytes.as_slice(), &[0]].concat()).is_err());
    }

    #[test]
    fn status_flags() {
        let mut status = BlockStatus::default();
        status.insert(BlockStatus::HEADER_VALID);
        status.insert(BlockStatus::FAILED);
        assert!(status.contains(BlockStatus::FAILED));
        status.remove(BlockStatus::FAILED);
        assert!(!status.contains(BlockStatus::FAILED));
        assert!(status.contains(BlockStatus::HEADER_VALID));
    }
}

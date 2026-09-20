//! In-memory tree of every known block header, with the most-work chain indexed by height.
//!
//! Only cheap checks run here: headers must connect, carry valid proof of work for the target
//! they claim, and not claim a target easier than the chain allows. Contextual rules such as
//! the difficulty adjustment and timestamps come with the consensus module.

use std::collections::HashMap;

use bitcoin::block::Header;
use bitcoin::blockdata::constants::genesis_block;
use bitcoin::consensus::params::Params;
use bitcoin::pow::Target;
use bitcoin::{BlockHash, Network};

use crate::storage::{BlockStatus, HeaderEntry};
use crate::validation::{self, ValidationError};

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum HeaderError {
    #[error("header {0} does not connect to a known header")]
    UnknownParent(BlockHash),

    #[error("header {0} does not follow the previous header in the message")]
    NotContinuous(BlockHash),

    #[error("header {0} does not meet its own proof-of-work target")]
    BadProofOfWork(BlockHash),

    #[error("header {0} claims a target easier than the chain allows")]
    TargetTooEasy(BlockHash),

    #[error("stored headers belong to another chain: genesis {0} is missing")]
    ForeignChain(BlockHash),

    #[error("header {hash} is invalid: {source}")]
    Invalid {
        hash: BlockHash,
        source: ValidationError,
    },
}

pub struct HeaderChain {
    max_target: Target,
    entries: HashMap<BlockHash, HeaderEntry>,
    /// Hashes of the most-work chain, indexed by height. `best[0]` is always genesis.
    best: Vec<BlockHash>,
}

impl HeaderChain {
    /// Builds the tree from stored entries. With no stored entries, the tree holds only the
    /// genesis header, which the caller should persist.
    pub fn new(network: Network, stored: Vec<HeaderEntry>) -> Result<Self, HeaderError> {
        let genesis = genesis_block(network).header;
        let genesis_hash = genesis.block_hash();
        let mut entries: HashMap<BlockHash, HeaderEntry> =
            stored.into_iter().map(|e| (e.block_hash(), e)).collect();

        if entries.is_empty() {
            let entry = HeaderEntry {
                header: genesis,
                height: 0,
                chain_work: genesis.work(),
                status: BlockStatus::default(),
            };
            entries.insert(genesis_hash, entry);
        } else if !entries.contains_key(&genesis_hash) {
            return Err(HeaderError::ForeignChain(genesis_hash));
        }

        let best_tip = entries
            .values()
            .max_by_key(|e| e.chain_work)
            .map(HeaderEntry::block_hash)
            .expect("the tree holds at least genesis");

        let mut chain = Self {
            max_target: Params::new(network).max_attainable_target,
            entries,
            best: vec![genesis_hash],
        };
        chain.set_best_tip(best_tip);
        Ok(chain)
    }

    pub fn tip(&self) -> &HeaderEntry {
        let hash = self
            .best
            .last()
            .expect("the best chain holds at least genesis");
        &self.entries[hash]
    }

    pub fn height(&self) -> u32 {
        self.tip().height
    }

    pub fn get(&self, hash: &BlockHash) -> Option<&HeaderEntry> {
        self.entries.get(hash)
    }

    pub fn contains(&self, hash: &BlockHash) -> bool {
        self.entries.contains_key(hash)
    }

    pub fn is_on_best_chain(&self, hash: &BlockHash) -> bool {
        self.entries
            .get(hash)
            .is_some_and(|e| self.best.get(e.height as usize) == Some(hash))
    }

    /// Block locator for `getheaders`: the tip, then hashes further back with doubling gaps,
    /// always ending at genesis. Mirrors Bitcoin Core `LocatorEntries`.
    pub fn locator(&self) -> Vec<BlockHash> {
        let mut hashes = Vec::new();
        let mut height = self.best.len() - 1;
        let mut step = 1;
        loop {
            hashes.push(self.best[height]);
            if height == 0 {
                return hashes;
            }
            if hashes.len() >= 10 {
                step *= 2;
            }
            height = height.saturating_sub(step);
        }
    }

    /// Validates a `headers` message and adds its new headers to the tree.
    ///
    /// Either every header is accepted or none is. Returns the entries that were not known
    /// before, in message order, so the caller can persist them.
    pub fn accept(&mut self, headers: &[Header]) -> Result<Vec<HeaderEntry>, HeaderError> {
        let mut new: Vec<HeaderEntry> = Vec::new();
        let mut prev: Option<HeaderEntry> = None;

        for header in headers {
            let hash = header.block_hash();
            let parent = match &prev {
                Some(entry) if header.prev_blockhash == entry.block_hash() => entry.clone(),
                Some(_) => return Err(HeaderError::NotContinuous(hash)),
                None => self
                    .entries
                    .get(&header.prev_blockhash)
                    .ok_or(HeaderError::UnknownParent(hash))?
                    .clone(),
            };
            let (parent_height, parent_work) = (parent.height, parent.chain_work);

            let (height, chain_work) = match self.entries.get(&hash) {
                Some(known) => (known.height, known.chain_work),
                None => {
                    self.check_pow(header, hash)?;
                    let entry = HeaderEntry {
                        header: *header,
                        height: parent_height + 1,
                        chain_work: parent_work + header.work(),
                        status: BlockStatus::default(),
                    };
                    // TODO: the contextual rules land with the consensus module.
                    validation::check_header(header, &parent)
                        .map_err(|source| HeaderError::Invalid { hash, source })?;
                    let key = (entry.height, entry.chain_work);
                    new.push(entry);
                    key
                }
            };
            prev = Some(HeaderEntry {
                header: *header,
                height,
                chain_work,
                status: BlockStatus::default(),
            });
        }

        for entry in &new {
            self.insert(entry.clone());
        }
        Ok(new)
    }

    /// Marks a header as having its block stored. Returns the updated entry to persist.
    pub fn mark_have_data(&mut self, hash: &BlockHash) -> Option<HeaderEntry> {
        let entry = self.entries.get_mut(hash)?;
        entry.status.insert(BlockStatus::HAVE_DATA);
        Some(entry.clone())
    }

    fn check_pow(&self, header: &Header, hash: BlockHash) -> Result<(), HeaderError> {
        if header.target() > self.max_target {
            return Err(HeaderError::TargetTooEasy(hash));
        }
        header
            .validate_pow(header.target())
            .map(|_| ())
            .map_err(|_| HeaderError::BadProofOfWork(hash))
    }

    fn insert(&mut self, entry: HeaderEntry) {
        let hash = entry.block_hash();
        let more_work = entry.chain_work > self.tip().chain_work;
        self.entries.insert(hash, entry);
        if more_work {
            self.set_best_tip(hash);
        }
    }

    /// Makes `tip` the end of the best chain, replacing the old branch above the fork point.
    fn set_best_tip(&mut self, tip: BlockHash) {
        let mut branch = Vec::new();
        let mut hash = tip;
        loop {
            let entry = &self.entries[&hash];
            if self.best.get(entry.height as usize) == Some(&hash) {
                break;
            }
            branch.push(hash);
            hash = entry.header.prev_blockhash;
        }
        let fork_height = self.entries[&hash].height as usize;
        self.best.truncate(fork_height + 1);
        self.best.extend(branch.into_iter().rev());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bitcoin::CompactTarget;
    use bitcoin::TxMerkleNode;
    use bitcoin::hashes::Hash;

    const REGTEST: Network = Network::Regtest;

    fn genesis(network: Network) -> Header {
        genesis_block(network).header
    }

    /// A header on top of `prev` with valid regtest proof of work. `tag` separates forks.
    fn mine(prev: &Header, tag: u8) -> Header {
        let mut header = Header {
            prev_blockhash: prev.block_hash(),
            merkle_root: TxMerkleNode::from_byte_array([tag; 32]),
            time: prev.time + 600,
            nonce: 0,
            ..*prev
        };
        while header.validate_pow(header.target()).is_err() {
            header.nonce += 1;
        }
        header
    }

    fn mine_chain(from: &Header, len: usize, tag: u8) -> Vec<Header> {
        let mut headers: Vec<Header> = Vec::new();
        for _ in 0..len {
            let prev = headers.last().unwrap_or(from);
            headers.push(mine(prev, tag));
        }
        headers
    }

    #[test]
    fn empty_store_starts_at_genesis() {
        let chain = HeaderChain::new(REGTEST, vec![]).unwrap();
        let genesis_hash = genesis(REGTEST).block_hash();
        assert_eq!(chain.height(), 0);
        assert_eq!(chain.tip().block_hash(), genesis_hash);
        assert_eq!(chain.locator(), vec![genesis_hash]);
    }

    #[test]
    fn accepts_a_chain_and_tracks_work() {
        let mut chain = HeaderChain::new(REGTEST, vec![]).unwrap();
        let headers = mine_chain(&genesis(REGTEST), 5, 1);

        let new = chain.accept(&headers).unwrap();
        assert_eq!(new.len(), 5);
        assert_eq!(chain.height(), 5);
        assert_eq!(chain.tip().block_hash(), headers[4].block_hash());
        let expected_work = headers
            .iter()
            .fold(genesis(REGTEST).work(), |work, h| work + h.work());
        assert_eq!(chain.tip().chain_work, expected_work);

        assert!(
            chain.accept(&headers).unwrap().is_empty(),
            "known headers are not new"
        );
    }

    #[test]
    fn rejects_whole_message_on_error() {
        let mut chain = HeaderChain::new(REGTEST, vec![]).unwrap();
        let headers = mine_chain(&genesis(REGTEST), 3, 1);

        let orphan = &headers[1..];
        assert_eq!(
            chain.accept(orphan),
            Err(HeaderError::UnknownParent(headers[1].block_hash()))
        );

        let gap = [headers[0], headers[2]];
        assert_eq!(
            chain.accept(&gap),
            Err(HeaderError::NotContinuous(headers[2].block_hash()))
        );

        let mut bad_pow = headers.clone();
        while bad_pow[2].validate_pow(bad_pow[2].target()).is_ok() {
            bad_pow[2].nonce += 1;
        }
        assert_eq!(
            chain.accept(&bad_pow),
            Err(HeaderError::BadProofOfWork(bad_pow[2].block_hash()))
        );

        assert_eq!(chain.height(), 0, "nothing from failed messages is kept");
    }

    #[test]
    fn rejects_targets_easier_than_the_chain_allows() {
        let mut chain = HeaderChain::new(Network::Bitcoin, vec![]).unwrap();
        let parent = genesis(Network::Bitcoin);
        // The target check runs before proof of work, so no mining is needed. Mining at
        // mainnet difficulty would take hours.
        let header = Header {
            prev_blockhash: parent.block_hash(),
            bits: CompactTarget::from_consensus(0x207f_ffff),
            ..parent
        };
        assert_eq!(
            chain.accept(&[header]),
            Err(HeaderError::TargetTooEasy(header.block_hash()))
        );
    }

    #[test]
    fn follows_the_most_work_fork() {
        let mut chain = HeaderChain::new(REGTEST, vec![]).unwrap();
        let root = genesis(REGTEST);
        let branch_a = mine_chain(&root, 3, 1);
        let branch_b = mine_chain(&root, 4, 2);

        chain.accept(&branch_a).unwrap();
        chain.accept(&branch_b).unwrap();
        assert_eq!(chain.tip().block_hash(), branch_b[3].block_hash());
        assert!(chain.is_on_best_chain(&branch_b[0].block_hash()));
        assert!(!chain.is_on_best_chain(&branch_a[0].block_hash()));

        let extension = mine_chain(&branch_a[2], 2, 1);
        chain.accept(&extension).unwrap();
        assert_eq!(chain.height(), 5);
        assert!(chain.is_on_best_chain(&branch_a[0].block_hash()));
        assert!(!chain.is_on_best_chain(&branch_b[3].block_hash()));
    }

    #[test]
    fn locator_doubles_gaps_and_ends_at_genesis() {
        let mut chain = HeaderChain::new(REGTEST, vec![]).unwrap();
        let headers = mine_chain(&genesis(REGTEST), 40, 1);
        chain.accept(&headers).unwrap();

        let heights: Vec<u32> = chain
            .locator()
            .iter()
            .map(|h| chain.get(h).unwrap().height)
            .collect();
        assert_eq!(
            heights,
            vec![40, 39, 38, 37, 36, 35, 34, 33, 32, 31, 29, 25, 17, 1, 0]
        );
    }

    #[test]
    fn rebuilds_best_chain_from_stored_entries() {
        let mut chain = HeaderChain::new(REGTEST, vec![]).unwrap();
        let root = genesis(REGTEST);
        chain.accept(&mine_chain(&root, 2, 1)).unwrap();
        let main = mine_chain(&root, 3, 2);
        chain.accept(&main).unwrap();
        let stored: Vec<HeaderEntry> = chain.entries.values().cloned().collect();

        let reloaded = HeaderChain::new(REGTEST, stored).unwrap();
        assert_eq!(reloaded.tip().block_hash(), main[2].block_hash());
        assert_eq!(reloaded.locator(), chain.locator());
    }

    #[test]
    fn refuses_headers_from_another_chain() {
        let signet = HeaderChain::new(Network::Signet, vec![]).unwrap();
        let stored = vec![signet.tip().clone()];
        assert_eq!(
            HeaderChain::new(REGTEST, stored).err(),
            Some(HeaderError::ForeignChain(genesis(REGTEST).block_hash()))
        );
    }
}

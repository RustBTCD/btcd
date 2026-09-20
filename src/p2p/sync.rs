//! Header sync and block download.
//!
//! Headers are synced from one peer at a time with repeated `getheaders`. Once that finishes,
//! blocks announced by any peer are fetched and stored. Blocks are not validated yet; see
//! `crate::validation`.

use std::collections::{HashSet, VecDeque};
use std::time::{Duration, Instant};

use bitcoin::block::Header;
use bitcoin::hashes::Hash;
use bitcoin::p2p::message::NetworkMessage;
use bitcoin::p2p::message_blockdata::{GetHeadersMessage, Inventory};
use bitcoin::{Block, BlockHash};
use tracing::{debug, info, warn};

use super::P2pError;
use super::connection::PeerId;
use super::manager::{BlockRequest, Manager, blocking};
use crate::header_chain::HeaderError;
use crate::storage::BlockStatus;
use crate::validation;

/// Bitcoin Core `MAX_HEADERS_RESULTS`. A full reply means more headers may follow.
const MAX_HEADERS: usize = 2000;
/// Bitcoin Core `MAX_BLOCKS_IN_TRANSIT_PER_PEER`.
const MAX_BLOCKS_IN_FLIGHT_PER_PEER: usize = 16;
/// Coinbase output prefix of a BIP141 witness commitment: OP_RETURN, push 36, 0xaa21a9ed.
const WITNESS_COMMITMENT_PREFIX: [u8; 6] = [0x6a, 0x24, 0xaa, 0x21, 0xa9, 0xed];

impl Manager {
    pub(super) fn start_header_sync(&mut self, id: PeerId) {
        info!(
            "peer {id}: syncing headers from height {}",
            self.headers.height()
        );
        self.sync_peer = Some(id);
        self.request_headers(id);
    }

    pub(super) fn request_headers(&mut self, id: PeerId) {
        let message = GetHeadersMessage::new(self.headers.locator(), BlockHash::all_zeros());
        self.send(id, NetworkMessage::GetHeaders(message));
        if self.sync_peer == Some(id) && self.synced_height.is_none() {
            self.sync_requested_at = Some(Instant::now());
        }
    }

    pub(super) async fn on_headers(
        &mut self,
        id: PeerId,
        headers: Vec<Header>,
    ) -> Result<(), P2pError> {
        if headers.len() > MAX_HEADERS {
            let reason = format!("sent {} headers, limit is {MAX_HEADERS}", headers.len());
            self.misbehaving(id, reason);
            return Ok(());
        }
        let full = headers.len() == MAX_HEADERS;
        if self.sync_peer == Some(id) {
            self.sync_requested_at = None;
        }

        let new = match self.headers.accept(&headers) {
            Ok(new) => new,
            Err(HeaderError::UnknownParent(_)) => {
                // We lack the ancestors. During initial sync the sync peer fills the gap, so
                // only ask for them once synced.
                if self.synced_height.is_some() {
                    self.request_headers(id);
                }
                return Ok(());
            }
            Err(err) => {
                self.misbehaving(id, err.to_string());
                return Ok(());
            }
        };

        if !new.is_empty() {
            let store = self.storage.headers();
            let entries = new.clone();
            blocking(move || store.put_many(&entries)).await?;
        }

        if self.synced_height.is_some() {
            for entry in &new {
                let hash = entry.block_hash();
                if self.headers.is_on_best_chain(&hash) {
                    info!("peer {id}: new header {hash} at height {}", entry.height);
                }
            }
            self.queue_missing_blocks();
            if full {
                self.request_headers(id);
            }
            self.request_blocks();
        } else if self.sync_peer == Some(id) {
            if full {
                info!("headers synced to height {}", self.headers.height());
                self.request_headers(id);
            } else {
                self.synced_height = Some(self.headers.height());
                info!(
                    "header sync complete at height {}, tip {}; waiting for new blocks",
                    self.headers.height(),
                    self.headers.tip().block_hash()
                );
            }
        }
        Ok(())
    }

    pub(super) fn on_inv(&mut self, id: PeerId, items: &[Inventory]) {
        if self.synced_height.is_none() {
            return;
        }
        let unknown_block = items.iter().any(|item| match item {
            Inventory::Block(hash)
            | Inventory::WitnessBlock(hash)
            | Inventory::CompactBlock(hash) => !self.headers.contains(hash),
            _ => false,
        });
        // Bitcoin Core does the same: an unknown block announcement triggers `getheaders`.
        if unknown_block {
            self.request_headers(id);
        }
    }

    /// Queues best-chain blocks above the sync height that are not stored yet.
    ///
    /// Walking back from the tip also covers reorgs: blocks of the new branch that arrived
    /// earlier, while that branch was not the best chain, get queued too.
    pub(super) fn queue_missing_blocks(&mut self) {
        let Some(floor) = self.synced_height else {
            return;
        };
        let mut missing = Vec::new();
        let mut entry = self.headers.tip();
        while entry.height > floor && !entry.status.contains(BlockStatus::HAVE_DATA) {
            let hash = entry.block_hash();
            if !self.in_flight.contains_key(&hash) && !self.wanted_blocks.contains(&hash) {
                missing.push(hash);
            }
            match self.headers.get(&entry.header.prev_blockhash) {
                Some(parent) => entry = parent,
                None => break,
            }
        }
        self.wanted_blocks.extend(missing.into_iter().rev());
    }

    /// Sends `getdata` for queued blocks to the least busy peers.
    pub(super) fn request_blocks(&mut self) {
        let mut waiting = VecDeque::new();
        while let Some(hash) = self.wanted_blocks.pop_front() {
            let stored = self
                .headers
                .get(&hash)
                .is_some_and(|e| e.status.contains(BlockStatus::HAVE_DATA));
            if stored || self.in_flight.contains_key(&hash) {
                continue;
            }
            let Some(peer_id) = self.pick_peer(self.not_found.get(&hash)) else {
                waiting.push_back(hash);
                continue;
            };

            // The witness variant; a plain block request returns the block without witnesses.
            let request = NetworkMessage::GetData(vec![Inventory::WitnessBlock(hash)]);
            self.send(peer_id, request);
            if let Some(peer) = self.peers.get_mut(&peer_id) {
                peer.blocks_in_flight += 1;
            }
            self.in_flight.insert(
                hash,
                BlockRequest {
                    peer: peer_id,
                    since: Instant::now(),
                },
            );
            debug!("peer {peer_id}: requested block {hash}");
        }
        self.wanted_blocks = waiting;
    }

    fn pick_peer(&self, excluded: Option<&HashSet<PeerId>>) -> Option<PeerId> {
        self.peers
            .iter()
            .filter(|(id, peer)| {
                peer.is_ready()
                    && peer.blocks_in_flight < MAX_BLOCKS_IN_FLIGHT_PER_PEER
                    && !excluded.is_some_and(|ex| ex.contains(id))
            })
            .min_by_key(|(id, peer)| (peer.blocks_in_flight, **id))
            .map(|(id, _)| *id)
    }

    pub(super) async fn on_block(&mut self, id: PeerId, block: Block) -> Result<(), P2pError> {
        let hash = block.block_hash();
        if !self.in_flight.get(&hash).is_some_and(|r| r.peer == id) {
            debug!("peer {id}: ignoring unrequested block {hash}");
            return Ok(());
        }
        self.finish_request(&hash);

        if let Err(reason) = check_block_body(&block) {
            self.misbehaving(id, format!("sent block {hash} that {reason}"));
            self.wanted_blocks.push_front(hash);
            self.request_blocks();
            return Ok(());
        }

        let Some(entry) = self.headers.get(&hash).cloned() else {
            return Ok(());
        };
        // TODO: consensus validation happens here once the validation module is written.
        if let Err(err) = validation::check_block(&block, &entry) {
            self.misbehaving(id, format!("sent invalid block {hash}: {err}"));
            return Ok(());
        }

        let Some(entry) = self.headers.mark_have_data(&hash) else {
            return Ok(());
        };
        self.not_found.remove(&hash);
        let (height, transactions, size) = (entry.height, block.txdata.len(), block.total_size());

        let blocks = self.storage.blocks();
        let headers = self.storage.headers();
        blocking(move || {
            blocks.put(&block)?;
            headers.put(&entry)
        })
        .await?;

        info!(
            "peer {id}: stored block {hash} at height {height}, {transactions} transactions, {size} bytes"
        );
        self.request_blocks();
        Ok(())
    }

    pub(super) fn on_not_found(&mut self, id: PeerId, items: &[Inventory]) {
        for item in items {
            let (Inventory::Block(hash) | Inventory::WitnessBlock(hash)) = item else {
                continue;
            };
            if !self.in_flight.get(hash).is_some_and(|r| r.peer == id) {
                continue;
            }
            self.finish_request(hash);
            self.not_found.entry(*hash).or_default().insert(id);
            self.wanted_blocks.push_front(*hash);
        }
        self.request_blocks();
    }

    fn finish_request(&mut self, hash: &BlockHash) {
        if let Some(request) = self.in_flight.remove(hash)
            && let Some(peer) = self.peers.get_mut(&request.peer)
        {
            peer.blocks_in_flight = peer.blocks_in_flight.saturating_sub(1);
        }
    }

    pub(super) fn requeue_requests_from(&mut self, id: PeerId) {
        let hashes: Vec<BlockHash> = self
            .in_flight
            .iter()
            .filter(|(_, r)| r.peer == id)
            .map(|(hash, _)| *hash)
            .collect();
        for hash in hashes {
            self.finish_request(&hash);
            self.wanted_blocks.push_front(hash);
        }
    }

    /// Disconnects peers that did not answer a header or block request in time.
    pub(super) fn expire_requests(&mut self) {
        let limit = Duration::from_secs(self.config.request_timeout_secs);

        if let (Some(id), Some(since)) = (self.sync_peer, self.sync_requested_at)
            && since.elapsed() > limit
        {
            warn!("peer {id}: stalled during header sync");
            self.sync_requested_at = None;
            self.disconnect(id, "stalled during header sync");
        }

        let stalled: HashSet<PeerId> = self
            .in_flight
            .values()
            .filter(|r| r.since.elapsed() > limit)
            .map(|r| r.peer)
            .collect();
        for id in stalled {
            warn!("peer {id}: stalled on block download");
            self.disconnect(id, "stalled on block download");
            self.requeue_requests_from(id);
        }
        self.request_blocks();
    }
}

/// Checks that a block body matches its header and still carries its witness data, so a peer
/// cannot hand us a different or stripped block. Full validation comes later.
fn check_block_body(block: &Block) -> Result<(), &'static str> {
    if !block.check_merkle_root() {
        return Err("does not match its header's merkle root");
    }
    if !block.check_witness_commitment() {
        return Err("does not match its witness commitment");
    }
    if is_witness_stripped(block) {
        return Err("is missing its witness data");
    }
    Ok(())
}

/// A block that commits to witness data must carry the 32-byte witness reserved value in its
/// coinbase. Without it, the witnesses were stripped. Mirrors Bitcoin Core's
/// `CheckWitnessMalleation`.
fn is_witness_stripped(block: &Block) -> bool {
    let Some(coinbase) = block.txdata.first() else {
        return false;
    };
    let has_commitment = coinbase.output.iter().any(|output| {
        output
            .script_pubkey
            .as_bytes()
            .starts_with(&WITNESS_COMMITMENT_PREFIX)
    });
    let has_reserved_value = coinbase.input.first().is_some_and(|input| {
        input.witness.len() == 1 && input.witness.nth(0).is_some_and(|item| item.len() == 32)
    });
    has_commitment && !has_reserved_value
}

#[cfg(test)]
mod tests {
    use super::*;
    use bitcoin::blockdata::constants::genesis_block;
    use bitcoin::{Network, Witness};

    /// A block with one SegWit spend and a valid witness commitment.
    fn segwit_block() -> Block {
        use bitcoin::transaction::Version;
        use bitcoin::{Amount, OutPoint, ScriptBuf, Sequence, Transaction, TxIn, TxOut, Txid};

        let input = |previous_output, script_sig: Vec<u8>, witness: Witness| TxIn {
            previous_output,
            script_sig: ScriptBuf::from_bytes(script_sig),
            sequence: Sequence::MAX,
            witness,
        };
        let tx = |input: TxIn, value| Transaction {
            version: Version::TWO,
            lock_time: bitcoin::absolute::LockTime::ZERO,
            input: vec![input],
            output: vec![TxOut {
                value: Amount::from_sat(value),
                script_pubkey: ScriptBuf::new(),
            }],
        };
        let reserved = [0u8; 32];
        let coinbase = tx(
            input(
                OutPoint::null(),
                vec![1, 1],
                Witness::from_slice(&[reserved]),
            ),
            50_0000_0000,
        );
        let spend_from = OutPoint {
            txid: Txid::all_zeros(),
            vout: 0,
        };
        let spend = tx(
            input(spend_from, vec![], Witness::from_slice(&[[7u8; 72]])),
            1_000,
        );

        let mut block = Block {
            header: genesis_block(Network::Regtest).header,
            txdata: vec![coinbase, spend],
        };
        let witness_root = block.witness_root().unwrap();
        let commitment = Block::compute_witness_commitment(&witness_root, &reserved);
        let mut script = WITNESS_COMMITMENT_PREFIX.to_vec();
        script.extend_from_slice(commitment.as_byte_array());
        block.txdata[0].output.push(bitcoin::TxOut {
            value: bitcoin::Amount::ZERO,
            script_pubkey: bitcoin::ScriptBuf::from_bytes(script),
        });
        block.header.merkle_root = block.compute_merkle_root().unwrap();
        block
    }

    #[test]
    fn genesis_block_body_is_accepted() {
        assert_eq!(check_block_body(&genesis_block(Network::Bitcoin)), Ok(()));
    }

    #[test]
    fn detects_blocks_that_do_not_match_their_header() {
        let mut block = genesis_block(Network::Bitcoin);
        block.txdata[0].output[0].value = bitcoin::Amount::from_sat(1);
        assert!(check_block_body(&block).is_err());
    }

    #[test]
    fn detects_stripped_witness_data() {
        let block = segwit_block();
        assert_eq!(check_block_body(&block), Ok(()));

        let mut stripped = block.clone();
        for tx in &mut stripped.txdata {
            for input in &mut tx.input {
                input.witness = Witness::new();
            }
        }
        assert_eq!(
            check_block_body(&stripped),
            Err("is missing its witness data")
        );

        let mut wrong_witness = block;
        wrong_witness.txdata[1].input[0].witness = Witness::from_slice(&[[8u8; 72]]);
        assert_eq!(
            check_block_body(&wrong_witness),
            Err("does not match its witness commitment")
        );
    }
}

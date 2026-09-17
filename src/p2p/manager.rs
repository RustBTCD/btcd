//! Owns all peer state and drives header sync and block download.
//!
//! Flow:
//! 1. Keep up to `max_outbound` connections, from the configured peers or DNS seeds.
//! 2. Sync headers from one peer with repeated `getheaders` until a reply is not full.
//! 3. After that, headers announced by any peer that extend the best chain are queued, and
//!    their blocks are fetched with `getdata` and stored. Blocks are not validated yet.

use std::collections::{HashMap, HashSet, VecDeque};
use std::net::{Ipv6Addr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};

use bitcoin::block::Header;
use bitcoin::hashes::Hash;
use bitcoin::p2p::message::NetworkMessage;
use bitcoin::p2p::message_blockdata::{GetHeadersMessage, Inventory};
use bitcoin::{Block, BlockHash};
use tokio::net::lookup_host;
use tokio::sync::mpsc;
use tokio::time::timeout;
use tracing::{debug, info, warn};

use super::P2pError;
use super::config::P2pConfig;
use super::peer::{self, PeerCommand, PeerContext, PeerEvent, PeerId, PeerInfo};
use crate::chain_params::Chain;
use crate::header_chain::{HeaderChain, HeaderError};
use crate::storage::{BlockStatus, Storage, StorageError};

/// Bitcoin Core `MAX_HEADERS_RESULTS`. A full reply means more headers may follow.
const MAX_HEADERS: usize = 2000;
/// Bitcoin Core `MAX_BLOCKS_IN_TRANSIT_PER_PEER`.
const MAX_BLOCKS_IN_FLIGHT_PER_PEER: usize = 16;
/// Buffer between peer tasks and the manager. When full, peers stop reading their sockets.
const EVENT_CHANNEL_CAPACITY: usize = 1024;
/// How often timeouts are checked and connections refilled.
const TICK: Duration = Duration::from_secs(5);
/// Coinbase output prefix of a BIP141 witness commitment: OP_RETURN, push 36, 0xaa21a9ed.
const WITNESS_COMMITMENT_PREFIX: [u8; 6] = [0x6a, 0x24, 0xaa, 0x21, 0xa9, 0xed];

struct ConnectedPeer {
    info: PeerInfo,
    commands: mpsc::UnboundedSender<PeerCommand>,
    blocks_in_flight: usize,
    /// Set once we asked the peer task to close; the peer gets no new requests.
    disconnecting: bool,
}

struct BlockRequest {
    peer: PeerId,
    since: Instant,
}

pub struct Manager {
    chain: Chain,
    config: P2pConfig,
    storage: Storage,
    headers: HeaderChain,
    peer_ctx: Arc<PeerContext>,
    events_tx: mpsc::Sender<PeerEvent>,
    events_rx: mpsc::Receiver<PeerEvent>,

    next_peer_id: PeerId,
    connecting: HashMap<PeerId, SocketAddr>,
    peers: HashMap<PeerId, ConnectedPeer>,
    candidates: VecDeque<SocketAddr>,
    last_resolve: Option<Instant>,

    sync_peer: Option<PeerId>,
    sync_requested_at: Option<Instant>,
    /// Best header height when initial header sync finished. Blocks above it are fetched.
    synced_height: Option<u32>,

    wanted_blocks: VecDeque<BlockHash>,
    in_flight: HashMap<BlockHash, BlockRequest>,
    /// Peers that answered `notfound` for a block; it is not requested from them again.
    not_found: HashMap<BlockHash, HashSet<PeerId>>,
}

impl Manager {
    /// Loads the header tree from storage, storing the genesis header on first start.
    pub async fn new(chain: Chain, config: P2pConfig, storage: Storage) -> Result<Self, P2pError> {
        let store = storage.headers();
        let stored = blocking(move || store.load_all()).await?;
        let first_start = stored.is_empty();
        let headers = HeaderChain::new(chain.network(), stored)?;
        if first_start {
            let genesis = headers.tip().clone();
            let store = storage.headers();
            blocking(move || store.put(&genesis)).await?;
        }
        info!(
            "header chain loaded at height {}, tip {}",
            headers.height(),
            headers.tip().block_hash()
        );

        let (events_tx, events_rx) = mpsc::channel(EVENT_CHANNEL_CAPACITY);
        Ok(Self {
            chain,
            peer_ctx: Arc::new(PeerContext::new(chain, &config)),
            config,
            storage,
            headers,
            events_tx,
            events_rx,
            next_peer_id: 0,
            connecting: HashMap::new(),
            peers: HashMap::new(),
            candidates: VecDeque::new(),
            last_resolve: None,
            sync_peer: None,
            sync_requested_at: None,
            synced_height: None,
            wanted_blocks: VecDeque::new(),
            in_flight: HashMap::new(),
            not_found: HashMap::new(),
        })
    }

    /// Runs until a storage error occurs. Peer failures are handled internally.
    pub async fn run(mut self) -> Result<(), P2pError> {
        self.fill_outbound().await;
        if self.connecting.is_empty() {
            return Err(P2pError::NoPeers);
        }

        let mut tick = tokio::time::interval(TICK);
        loop {
            tokio::select! {
                Some(event) = self.events_rx.recv() => self.handle_event(event).await?,
                _ = tick.tick() => {
                    self.expire_requests();
                    self.fill_outbound().await;
                }
            }
        }
    }

    async fn handle_event(&mut self, event: PeerEvent) -> Result<(), P2pError> {
        match event {
            PeerEvent::Connected { id, info, commands } => self.on_connected(id, info, commands),
            PeerEvent::Message { id, message } => self.on_message(id, message).await?,
            PeerEvent::Disconnected { id, reason } => self.on_disconnected(id, &reason),
        }
        Ok(())
    }

    // Connections

    async fn fill_outbound(&mut self) {
        while self.connecting.len() + self.peers.len() < self.config.max_outbound {
            if self.candidates.is_empty() && !self.refill_candidates().await {
                break;
            }
            let Some(addr) = self.candidates.pop_front() else {
                break;
            };
            if self.is_connected_to(addr) {
                continue;
            }
            let id = self.next_peer_id;
            self.next_peer_id += 1;
            self.connecting.insert(id, addr);
            debug!("peer {id}: connecting to {addr}");
            peer::spawn(
                id,
                addr,
                self.headers.height(),
                self.peer_ctx.clone(),
                self.events_tx.clone(),
            );
        }
    }

    /// Looks up peer addresses again, at most once per `retry_interval_secs`.
    async fn refill_candidates(&mut self) -> bool {
        let retry = Duration::from_secs(self.config.retry_interval_secs);
        if self.last_resolve.is_some_and(|t| t.elapsed() < retry) {
            return false;
        }
        self.last_resolve = Some(Instant::now());
        let lookup_timeout = Duration::from_secs(self.config.connect_timeout_secs);
        let addrs = resolve_peers(self.chain, &self.config.peers, lookup_timeout).await;
        if addrs.is_empty() {
            warn!("no peer addresses found");
        }
        self.candidates.extend(addrs);
        !self.candidates.is_empty()
    }

    fn is_connected_to(&self, addr: SocketAddr) -> bool {
        self.connecting.values().any(|a| *a == addr)
            || self.peers.values().any(|p| p.info.addr == addr)
    }

    fn on_connected(
        &mut self,
        id: PeerId,
        info: PeerInfo,
        commands: mpsc::UnboundedSender<PeerCommand>,
    ) {
        self.connecting.remove(&id);
        info!(
            "peer {id}: connected to {} ({}, version {}, height {}, {})",
            info.addr, info.user_agent, info.version, info.start_height, info.services
        );
        let peer = ConnectedPeer {
            info,
            commands,
            blocks_in_flight: 0,
            disconnecting: false,
        };
        self.peers.insert(id, peer);

        if self.synced_height.is_none() && self.sync_peer.is_none() {
            self.start_header_sync(id);
        }
        self.request_blocks();
    }

    fn on_disconnected(&mut self, id: PeerId, reason: &str) {
        if let Some(addr) = self.connecting.remove(&id) {
            debug!("peer {id}: could not connect to {addr}: {reason}");
            return;
        }
        let Some(peer) = self.peers.remove(&id) else {
            return;
        };
        info!("peer {id}: disconnected from {}: {reason}", peer.info.addr);

        self.requeue_requests_from(id);
        for peers in self.not_found.values_mut() {
            peers.remove(&id);
        }
        if self.sync_peer == Some(id) {
            self.sync_peer = None;
            self.sync_requested_at = None;
            let next = self.peers.keys().min().copied();
            if let (None, Some(next)) = (self.synced_height, next) {
                self.start_header_sync(next);
            }
        }
        self.request_blocks();
    }

    fn send(&self, id: PeerId, message: NetworkMessage) {
        if let Some(peer) = self.peers.get(&id) {
            let _ = peer.commands.send(PeerCommand::Send(message));
        }
    }

    fn disconnect(&mut self, id: PeerId, reason: impl Into<String>) {
        if let Some(peer) = self.peers.get_mut(&id) {
            peer.disconnecting = true;
            let _ = peer.commands.send(PeerCommand::Disconnect(reason.into()));
        }
    }

    // Headers

    fn start_header_sync(&mut self, id: PeerId) {
        info!(
            "peer {id}: syncing headers from height {}",
            self.headers.height()
        );
        self.sync_peer = Some(id);
        self.request_headers(id);
    }

    fn request_headers(&mut self, id: PeerId) {
        let message = GetHeadersMessage::new(self.headers.locator(), BlockHash::all_zeros());
        self.send(id, NetworkMessage::GetHeaders(message));
        if self.sync_peer == Some(id) && self.synced_height.is_none() {
            self.sync_requested_at = Some(Instant::now());
        }
    }

    async fn on_message(&mut self, id: PeerId, message: NetworkMessage) -> Result<(), P2pError> {
        match message {
            NetworkMessage::Headers(headers) => self.on_headers(id, headers).await?,
            NetworkMessage::Block(block) => self.on_block(id, block).await?,
            NetworkMessage::Inv(items) => self.on_inv(id, &items),
            NetworkMessage::NotFound(items) => self.on_not_found(id, &items),
            _ => {}
        }
        Ok(())
    }

    async fn on_headers(&mut self, id: PeerId, headers: Vec<Header>) -> Result<(), P2pError> {
        if headers.len() > MAX_HEADERS {
            let reason = format!("sent {} headers, limit is {MAX_HEADERS}", headers.len());
            self.disconnect(id, reason);
            return Ok(());
        }
        let full = headers.len() == MAX_HEADERS;
        if self.sync_peer == Some(id) {
            self.sync_requested_at = None;
        }

        let new = match self.headers.accept(&headers) {
            Ok(new) => new,
            Err(HeaderError::UnknownParent(_)) => {
                // We lack the ancestors, e.g. after missing an announcement. During initial
                // sync the sync peer fills the gap, so only ask once synced.
                if self.synced_height.is_some() {
                    self.request_headers(id);
                }
                return Ok(());
            }
            Err(err) => {
                warn!("peer {id}: invalid headers: {err}");
                self.disconnect(id, err.to_string());
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

    fn on_inv(&mut self, id: PeerId, items: &[Inventory]) {
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

    // Blocks

    /// Queues best-chain blocks above the sync height that are not stored yet.
    ///
    /// Walking back from the tip also covers reorgs: blocks of the new branch that arrived
    /// earlier, while that branch was not the best chain, get queued too.
    fn queue_missing_blocks(&mut self) {
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
    fn request_blocks(&mut self) {
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
            let request = BlockRequest {
                peer: peer_id,
                since: Instant::now(),
            };
            self.in_flight.insert(hash, request);
            debug!("peer {peer_id}: requested block {hash}");
        }
        self.wanted_blocks = waiting;
    }

    fn pick_peer(&self, excluded: Option<&HashSet<PeerId>>) -> Option<PeerId> {
        self.peers
            .iter()
            .filter(|(id, p)| {
                !p.disconnecting
                    && p.blocks_in_flight < MAX_BLOCKS_IN_FLIGHT_PER_PEER
                    && !excluded.is_some_and(|ex| ex.contains(id))
            })
            .min_by_key(|(id, p)| (p.blocks_in_flight, **id))
            .map(|(id, _)| *id)
    }

    async fn on_block(&mut self, id: PeerId, block: Block) -> Result<(), P2pError> {
        let hash = block.block_hash();
        if !self.in_flight.get(&hash).is_some_and(|r| r.peer == id) {
            debug!("peer {id}: ignoring unrequested block {hash}");
            return Ok(());
        }
        self.finish_request(&hash);

        if let Err(reason) = check_block_body(&block) {
            warn!("peer {id}: block {hash} rejected: {reason}");
            self.disconnect(id, format!("sent block {hash} that {reason}"));
            self.wanted_blocks.push_front(hash);
            self.request_blocks();
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

    fn on_not_found(&mut self, id: PeerId, items: &[Inventory]) {
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

    fn requeue_requests_from(&mut self, id: PeerId) {
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
    fn expire_requests(&mut self) {
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

/// Checks that a block body matches its header and still carries its witness data, so a
/// peer cannot hand us a different or stripped block. Full validation comes later.
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

/// Resolves the configured peers, or the chain's DNS seeds when none are configured.
/// Addresses from different hosts are interleaved so connections spread across sources.
async fn resolve_peers(
    chain: Chain,
    peers: &[String],
    lookup_timeout: Duration,
) -> Vec<SocketAddr> {
    let port = chain.default_port();
    let hosts: Vec<String> = if peers.is_empty() {
        chain
            .dns_seeds()
            .iter()
            .map(|seed| format!("{seed}:{port}"))
            .collect()
    } else {
        peers
            .iter()
            .map(|peer| with_default_port(peer, port))
            .collect()
    };

    let mut per_host = Vec::new();
    for host in hosts {
        match timeout(lookup_timeout, lookup_host(host.as_str())).await {
            Ok(Ok(addrs)) => per_host.push(addrs.collect::<VecDeque<_>>()),
            Ok(Err(err)) => warn!("resolving {host}: {err}"),
            Err(_) => warn!("resolving {host}: timed out"),
        }
    }

    let mut seen = HashSet::new();
    let mut result = Vec::new();
    while per_host.iter().any(|addrs| !addrs.is_empty()) {
        for addrs in &mut per_host {
            if let Some(addr) = addrs.pop_front()
                && seen.insert(addr)
            {
                result.push(addr);
            }
        }
    }
    result
}

/// Appends the chain's default port unless `peer` already names one.
fn with_default_port(peer: &str, port: u16) -> String {
    if peer.parse::<SocketAddr>().is_ok() {
        return peer.to_string();
    }
    if peer.parse::<Ipv6Addr>().is_ok() {
        return format!("[{peer}]:{port}");
    }
    let has_port = peer
        .rsplit_once(':')
        .is_some_and(|(host, p)| !host.contains(':') && p.parse::<u16>().is_ok());
    if has_port {
        peer.to_string()
    } else {
        format!("{peer}:{port}")
    }
}

/// Runs synchronous storage work on tokio's blocking pool.
async fn blocking<T, F>(f: F) -> Result<T, P2pError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, StorageError> + Send + 'static,
{
    Ok(tokio::task::spawn_blocking(f).await??)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bitcoin::blockdata::constants::genesis_block;
    use bitcoin::{Network, Witness};

    #[test]
    fn default_ports_are_added_only_when_missing() {
        assert_eq!(with_default_port("1.2.3.4", 8333), "1.2.3.4:8333");
        assert_eq!(with_default_port("1.2.3.4:18444", 8333), "1.2.3.4:18444");
        assert_eq!(with_default_port("node.example", 8333), "node.example:8333");
        assert_eq!(
            with_default_port("node.example:9000", 8333),
            "node.example:9000"
        );
        assert_eq!(with_default_port("::1", 8333), "[::1]:8333");
        assert_eq!(with_default_port("[::1]:9000", 8333), "[::1]:9000");
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

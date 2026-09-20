//! Connection policy and the event loop.
//!
//! This is Bitcoin Core's `CConnman` role: decide whom to connect to, keep the target number
//! of connections, drop peers that misbehave or go quiet, and hand messages to the protocol
//! layer. Protocol handling lives in `processing.rs`, chain sync in `sync.rs`.
//!
//! Messages are handled round robin, one per peer per round, as Core does, so a peer that
//! floods us cannot starve the others.

use std::collections::{HashMap, HashSet, VecDeque};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use bitcoin::BlockHash;
use bitcoin::p2p::ServiceFlags;
use bitcoin::p2p::message::NetworkMessage;
use tokio::net::lookup_host;
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::{OwnedSemaphorePermit, mpsc};
use tokio::time::timeout;
use tracing::{debug, info, warn};

use super::P2pError;
use super::addrman::{AddressManager, Group, group_of};
use super::banman::BanManager;
use super::config::P2pConfig;
use super::connection::{self, ConnectionCommand, ConnectionConfig, ConnectionEvent, PeerId};
use super::permissions::{PermissionTable, Permissions, Subnet};
use super::transport::TransportKind;
use crate::chain_params::Chain;
use crate::header_chain::HeaderChain;
use crate::storage::Storage;

/// Buffer between connection tasks and the manager.
const EVENT_CHANNEL_CAPACITY: usize = 1024;
/// How often timeouts are checked, connections refilled and state saved.
const TICK: Duration = Duration::from_secs(5);
/// Below this many known addresses we ask the DNS seeds again.
const MIN_ADDRESSES: usize = 64;

/// Why a connection was opened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ConnectionKind {
    /// Configured peer, always reconnected. Bitcoin Core's `-addnode`.
    Manual,
    /// Chosen from the address book.
    Automatic,
    /// Short connection that only tests whether an untried address works.
    Feeler,
}

/// Where a peer is in the version handshake.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Phase {
    AwaitingVersion,
    AwaitingVerack,
    Ready,
}

pub(super) struct Peer {
    pub addr: SocketAddr,
    pub kind: ConnectionKind,
    pub transport: TransportKind,
    pub permissions: Permissions,
    pub commands: mpsc::Sender<ConnectionCommand>,
    pub phase: Phase,
    /// Nonce we sent, to detect connecting to ourselves.
    pub nonce: u64,
    pub connected_at: Instant,
    pub version: u32,
    pub services: ServiceFlags,
    pub user_agent: String,
    pub start_height: i32,
    pub next_ping: Instant,
    pub ping_sent: Option<(u64, Instant)>,
    pub latency: Option<Duration>,
    /// Token bucket limiting how many addresses a peer may send us.
    pub addr_tokens: f64,
    pub addr_tokens_updated: Instant,
    pub blocks_in_flight: usize,
    pub disconnecting: bool,
}

impl Peer {
    pub fn is_ready(&self) -> bool {
        self.phase == Phase::Ready && !self.disconnecting
    }
}

struct Connecting {
    addr: SocketAddr,
    kind: ConnectionKind,
}

pub(super) struct BlockRequest {
    pub peer: PeerId,
    pub since: Instant,
}

pub struct Manager {
    pub(super) chain: Chain,
    pub(super) config: P2pConfig,
    pub(super) storage: Storage,
    pub(super) headers: HeaderChain,
    pub(super) addrman: AddressManager,
    pub(super) bans: BanManager,
    permissions: PermissionTable,
    conn_config: Arc<ConnectionConfig>,

    events_tx: mpsc::Sender<ConnectionEvent>,
    events_rx: mpsc::Receiver<ConnectionEvent>,
    pub(super) peers: HashMap<PeerId, Peer>,
    connecting: HashMap<PeerId, Connecting>,
    /// Messages waiting to be handled, one queue per peer.
    inbox: HashMap<PeerId, VecDeque<(NetworkMessage, OwnedSemaphorePermit)>>,
    /// Peer order for the round-robin loop; rotated every round.
    order: VecDeque<PeerId>,
    next_peer_id: PeerId,

    manual_addrs: Vec<SocketAddr>,
    last_lookup: Option<Instant>,
    last_feeler: Instant,

    pub(super) sync_peer: Option<PeerId>,
    pub(super) sync_requested_at: Option<Instant>,
    pub(super) synced_height: Option<u32>,
    pub(super) wanted_blocks: VecDeque<BlockHash>,
    pub(super) in_flight: HashMap<BlockHash, BlockRequest>,
    pub(super) not_found: HashMap<BlockHash, HashSet<PeerId>>,
}

impl Manager {
    pub async fn new(chain: Chain, config: P2pConfig, storage: Storage) -> Result<Self, P2pError> {
        let permissions = PermissionTable::parse(&config.whitelist)?;
        let mut configured_bans = Vec::new();
        for entry in &config.bans {
            configured_bans.push(Subnet::parse(entry)?);
        }

        let header_store = storage.headers();
        let stored_headers = blocking(move || header_store.load_all()).await?;
        let first_start = stored_headers.is_empty();
        let headers = HeaderChain::new(chain.network(), stored_headers)?;
        if first_start {
            let genesis = headers.tip().clone();
            let store = storage.headers();
            blocking(move || store.put(&genesis)).await?;
        }

        let address_store = storage.addresses();
        let stored_addresses = blocking(move || address_store.load_all()).await?;
        let ban_store = storage.bans();
        let stored_bans = blocking(move || ban_store.load_all()).await?;

        info!(
            "header chain loaded at height {}, tip {}; {} known addresses, {} stored bans",
            headers.height(),
            headers.tip().block_hash(),
            stored_addresses.len(),
            stored_bans.len()
        );

        let conn_config = Arc::new(ConnectionConfig {
            network: chain.network(),
            policy: config.transport,
            connect_timeout: Duration::from_secs(config.connect_timeout_secs),
            inactivity_timeout: Duration::from_secs(config.inactivity_timeout_secs),
            send_queue: config.send_queue,
            recv_quota: config.recv_quota,
        });
        let (events_tx, events_rx) = mpsc::channel(EVENT_CHANNEL_CAPACITY);

        Ok(Self {
            chain,
            addrman: AddressManager::new(stored_addresses, config.address_book_max),
            bans: BanManager::new(
                stored_bans,
                configured_bans,
                config.ban_duration_secs,
                config.discourage_duration_secs,
            ),
            permissions,
            conn_config,
            config,
            storage,
            headers,
            events_tx,
            events_rx,
            peers: HashMap::new(),
            connecting: HashMap::new(),
            inbox: HashMap::new(),
            order: VecDeque::new(),
            next_peer_id: 0,
            manual_addrs: Vec::new(),
            last_lookup: None,
            last_feeler: Instant::now(),
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
        self.resolve_manual_peers().await;
        self.seed_addresses().await;
        if self.manual_addrs.is_empty() && self.addrman.is_empty() {
            return Err(P2pError::NoPeers);
        }
        self.fill_connections();

        let mut tick = tokio::time::interval(TICK);
        loop {
            tokio::select! {
                Some(event) = self.events_rx.recv() => {
                    self.handle_event(event);
                    self.process_inbox().await?;
                }
                _ = tick.tick() => self.on_tick().await?,
            }
        }
    }

    fn handle_event(&mut self, event: ConnectionEvent) {
        match event {
            ConnectionEvent::Connected { id, kind, commands } => {
                self.on_connected(id, kind, commands);
            }
            ConnectionEvent::Message {
                id,
                message,
                permit,
            } => {
                if let Some(queue) = self.inbox.get_mut(&id) {
                    queue.push_back((message, permit));
                }
            }
            ConnectionEvent::Closed { id, reason } => self.on_closed(id, &reason),
        }
    }

    /// Handles at most one message per peer per round, as Bitcoin Core does.
    async fn process_inbox(&mut self) -> Result<(), P2pError> {
        loop {
            while let Ok(event) = self.events_rx.try_recv() {
                self.handle_event(event);
            }

            let mut handled = false;
            for _ in 0..self.order.len() {
                let Some(id) = self.order.pop_front() else {
                    break;
                };
                self.order.push_back(id);

                let next = self.inbox.get_mut(&id).and_then(VecDeque::pop_front);
                if let Some((message, permit)) = next {
                    handled = true;
                    self.on_message(id, message).await?;
                    // Releasing the permit here lets the peer's socket be read again.
                    drop(permit);
                }
            }
            if !handled {
                return Ok(());
            }
        }
    }

    async fn on_tick(&mut self) -> Result<(), P2pError> {
        let now = unix_now();
        self.bans.sweep(now);
        self.check_handshake_timeouts();
        self.send_pings();
        self.expire_requests();

        if self.wants_more_addresses() {
            self.seed_addresses().await;
        }
        self.open_feeler();
        self.fill_connections();
        self.save_network_state().await
    }

    // Connections

    fn fill_connections(&mut self) {
        let now = unix_now();
        for addr in self.manual_addrs.clone() {
            if !self.is_connected_to(addr) {
                self.connect_to(addr, ConnectionKind::Manual);
            }
        }

        while self.automatic_count() < self.config.max_outbound {
            let in_use = self.addresses_in_use();
            let groups = self.groups_in_use();
            let bans = &self.bans;
            let selected = self
                .addrman
                .select(false, now, &in_use, &groups, |ip| bans.is_blocked(ip, now));
            match selected {
                Some(addr) => self.connect_to(addr, ConnectionKind::Automatic),
                None => break,
            }
        }
    }

    fn open_feeler(&mut self) {
        let interval = Duration::from_secs(self.config.feeler_interval_secs);
        if self.config.max_outbound == 0 || self.last_feeler.elapsed() < interval {
            return;
        }
        self.last_feeler = Instant::now();

        let now = unix_now();
        let in_use = self.addresses_in_use();
        let bans = &self.bans;
        // Feelers ignore group diversity: they exist to test addresses, not to sync.
        let selected = self
            .addrman
            .select(true, now, &in_use, &HashSet::new(), |ip| {
                bans.is_blocked(ip, now)
            });
        if let Some(addr) = selected {
            debug!("feeler connection to {addr}");
            self.connect_to(addr, ConnectionKind::Feeler);
        }
    }

    fn connect_to(&mut self, addr: SocketAddr, kind: ConnectionKind) {
        let id = self.next_peer_id;
        self.next_peer_id += 1;
        self.addrman.mark_attempt(addr, unix_now());
        self.connecting.insert(id, Connecting { addr, kind });
        debug!("peer {id}: connecting to {addr} ({kind:?})");
        connection::spawn(id, addr, self.conn_config.clone(), self.events_tx.clone());
    }

    fn on_connected(
        &mut self,
        id: PeerId,
        transport: TransportKind,
        commands: mpsc::Sender<ConnectionCommand>,
    ) {
        let Some(Connecting { addr, kind }) = self.connecting.remove(&id) else {
            return;
        };
        let now = Instant::now();
        let peer = Peer {
            addr,
            kind,
            transport,
            permissions: self.permissions.for_peer(addr),
            commands,
            phase: Phase::AwaitingVersion,
            nonce: 0,
            connected_at: now,
            version: 0,
            services: ServiceFlags::NONE,
            user_agent: String::new(),
            start_height: 0,
            next_ping: now + Duration::from_secs(self.config.ping_interval_secs),
            ping_sent: None,
            latency: None,
            addr_tokens: 1.0,
            addr_tokens_updated: now,
            blocks_in_flight: 0,
            disconnecting: false,
        };
        self.peers.insert(id, peer);
        self.order.push_back(id);
        self.inbox.insert(id, VecDeque::new());
        debug!("peer {id}: {transport} transport to {addr}, sending version");
        self.send_version(id);
    }

    fn on_closed(&mut self, id: PeerId, reason: &str) {
        if let Some(Connecting { addr, .. }) = self.connecting.remove(&id) {
            self.addrman.mark_failed(addr, unix_now());
            debug!("peer {id}: could not connect to {addr}: {reason}");
            return;
        }
        let Some(peer) = self.peers.remove(&id) else {
            return;
        };
        self.order.retain(|other| *other != id);
        self.inbox.remove(&id);
        if peer.phase != Phase::Ready {
            self.addrman.mark_failed(peer.addr, unix_now());
        }
        info!("peer {id}: disconnected from {}: {reason}", peer.addr);

        self.requeue_requests_from(id);
        for peers in self.not_found.values_mut() {
            peers.remove(&id);
        }
        if self.sync_peer == Some(id) {
            self.sync_peer = None;
            self.sync_requested_at = None;
            let next = self
                .peers
                .iter()
                .find(|(_, p)| p.is_ready())
                .map(|(id, _)| *id);
            if let (None, Some(next)) = (self.synced_height, next) {
                self.start_header_sync(next);
            }
        }
        self.request_blocks();
        self.fill_connections();
    }

    fn check_handshake_timeouts(&mut self) {
        let limit = Duration::from_secs(self.config.handshake_timeout_secs);
        let stuck: Vec<PeerId> = self
            .peers
            .iter()
            .filter(|(_, p)| p.phase != Phase::Ready && p.connected_at.elapsed() > limit)
            .map(|(id, _)| *id)
            .collect();
        for id in stuck {
            self.disconnect(id, "handshake timed out");
        }
    }

    // Peer helpers used by the protocol and sync code

    pub(super) fn send(&mut self, id: PeerId, message: NetworkMessage) {
        let Some(peer) = self.peers.get(&id) else {
            return;
        };
        match peer.commands.try_send(ConnectionCommand::Send(message)) {
            Ok(()) => {}
            // Bitcoin Core pauses processing for a slow peer; we drop it instead.
            Err(TrySendError::Full(_)) => self.disconnect(id, "send queue is full"),
            Err(TrySendError::Closed(_)) => {}
        }
    }

    pub(super) fn disconnect(&mut self, id: PeerId, reason: impl Into<String>) {
        let reason = reason.into();
        if let Some(peer) = self.peers.get_mut(&id) {
            peer.disconnecting = true;
            let _ = peer
                .commands
                .try_send(ConnectionCommand::Disconnect(reason));
        }
    }

    /// One strike, as in Bitcoin Core: the peer is dropped and its address avoided.
    pub(super) fn misbehaving(&mut self, id: PeerId, reason: impl Into<String>) {
        let reason = reason.into();
        let Some(peer) = self.peers.get(&id) else {
            return;
        };
        let addr = peer.addr;
        if peer.permissions.contains(Permissions::NO_BAN) {
            warn!("peer {id} at {addr} misbehaved ({reason}), but has the noban permission");
            return;
        }
        warn!("peer {id} at {addr} misbehaved: {reason}");
        self.bans.discourage(addr.ip(), unix_now());
        self.disconnect(id, reason);
    }

    fn automatic_count(&self) -> usize {
        let connecting = self
            .connecting
            .values()
            .filter(|c| c.kind == ConnectionKind::Automatic)
            .count();
        let connected = self
            .peers
            .values()
            .filter(|p| p.kind == ConnectionKind::Automatic && !p.disconnecting)
            .count();
        connecting + connected
    }

    fn is_connected_to(&self, addr: SocketAddr) -> bool {
        self.connecting.values().any(|c| c.addr == addr)
            || self.peers.values().any(|p| p.addr == addr)
    }

    fn addresses_in_use(&self) -> HashSet<SocketAddr> {
        self.connecting
            .values()
            .map(|c| c.addr)
            .chain(self.peers.values().map(|p| p.addr))
            .collect()
    }

    /// Network groups of automatic connections, so we spread across the network.
    fn groups_in_use(&self) -> HashSet<Group> {
        self.connecting
            .values()
            .filter(|c| c.kind == ConnectionKind::Automatic)
            .map(|c| group_of(c.addr.ip()))
            .chain(
                self.peers
                    .values()
                    .filter(|p| p.kind == ConnectionKind::Automatic)
                    .map(|p| group_of(p.addr.ip())),
            )
            .collect()
    }

    // Address discovery

    async fn resolve_manual_peers(&mut self) {
        let port = self.chain.default_port();
        let hosts: Vec<String> = self
            .config
            .peers
            .iter()
            .map(|peer| with_default_port(peer, port))
            .collect();
        let lookup_timeout = Duration::from_secs(self.config.connect_timeout_secs);
        self.manual_addrs = resolve_hosts(&hosts, lookup_timeout).await;
        let now = unix_now();
        for addr in self.manual_addrs.clone() {
            self.addrman.add(addr, 0, now);
        }
    }

    fn wants_more_addresses(&self) -> bool {
        self.config.use_dns_seeds
            && self.addrman.len() < MIN_ADDRESSES
            && self.automatic_count() < self.config.max_outbound
    }

    /// Fills the address book from the chain's DNS seeds, at most once per retry interval.
    async fn seed_addresses(&mut self) {
        if !self.config.use_dns_seeds || self.chain.dns_seeds().is_empty() {
            return;
        }
        let retry = Duration::from_secs(self.config.retry_interval_secs);
        if self.last_lookup.is_some_and(|at| at.elapsed() < retry) {
            return;
        }
        self.last_lookup = Some(Instant::now());

        let port = self.chain.default_port();
        let hosts: Vec<String> = self
            .chain
            .dns_seeds()
            .iter()
            .map(|seed| format!("{seed}:{port}"))
            .collect();
        let lookup_timeout = Duration::from_secs(self.config.connect_timeout_secs);
        let addrs = resolve_hosts(&hosts, lookup_timeout).await;

        let now = unix_now();
        let added = addrs
            .into_iter()
            .filter(|addr| self.addrman.add(*addr, 0, now))
            .count();
        if added > 0 {
            info!(
                "added {added} addresses from DNS seeds, {} known",
                self.addrman.len()
            );
        }
    }

    /// Writes address book and ban changes to the database.
    async fn save_network_state(&mut self) -> Result<(), P2pError> {
        let addresses = self.addrman.take_dirty();
        let dropped = self.addrman.take_removed();
        let bans = self.bans.take_dirty();
        let unbanned = self.bans.take_removed();
        if addresses.is_empty() && dropped.is_empty() && bans.is_empty() && unbanned.is_empty() {
            return Ok(());
        }

        let address_store = self.storage.addresses();
        let ban_store = self.storage.bans();
        blocking(move || {
            address_store.put_many(&addresses)?;
            address_store.delete_many(&dropped)?;
            for ban in &bans {
                ban_store.put(ban)?;
            }
            for subnet in &unbanned {
                ban_store.delete(subnet)?;
            }
            Ok(())
        })
        .await
    }
}

/// Resolves hosts, interleaving results so connections spread across sources.
async fn resolve_hosts(hosts: &[String], lookup_timeout: Duration) -> Vec<SocketAddr> {
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

/// Appends the chain's default port unless the peer already names one.
pub(super) fn with_default_port(peer: &str, port: u16) -> String {
    use std::net::Ipv6Addr;
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

pub(super) fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// Runs synchronous storage work on tokio's blocking pool.
pub(super) async fn blocking<T, F>(f: F) -> Result<T, P2pError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, crate::storage::StorageError> + Send + 'static,
{
    Ok(tokio::task::spawn_blocking(f).await??)
}

#[cfg(test)]
mod tests {
    use super::*;

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
}

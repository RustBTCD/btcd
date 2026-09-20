//! Protocol handling: the version handshake, keepalive and address relay.
//!
//! This is Bitcoin Core's `net_processing` role. The handshake is a state machine driven by
//! messages, so it needs no socket and can be tested directly.

use std::time::{Duration, Instant};

use bitcoin::p2p::address::{AddrV2, AddrV2Message, Address};
use bitcoin::p2p::message::NetworkMessage;
use bitcoin::p2p::message_network::VersionMessage;
use bitcoin::p2p::{PROTOCOL_VERSION as CRATE_PROTOCOL_VERSION, ServiceFlags};
use bitcoin::secp256k1::rand::{Rng, thread_rng};
use std::net::SocketAddr;
use tracing::{debug, info};

use super::P2pError;
use super::connection::PeerId;
use super::manager::{ConnectionKind, Manager, Phase, unix_now};

/// Protocol version we speak. 70016 allows `wtxidrelay` (BIP339), which we accept and ignore.
pub(super) const PROTOCOL_VERSION: u32 = 70016;
/// Oldest peer version we accept. 70012 added `sendheaders` (BIP130), which we rely on.
const MIN_PEER_VERSION: u32 = 70012;
/// We understand witness data but serve nothing.
const OUR_SERVICES: ServiceFlags = ServiceFlags::WITNESS;
/// Bitcoin Core `MAX_ADDR_TO_SEND`.
const MAX_ADDR_PER_MESSAGE: usize = 1000;
/// Address messages allowed per second, averaged, as in Bitcoin Core.
const ADDR_TOKENS_PER_SECOND: f64 = 0.1;
/// Burst of addresses a peer may send at once.
const ADDR_TOKEN_MAX: f64 = MAX_ADDR_PER_MESSAGE as f64;

impl Manager {
    /// Starts the handshake by sending our version message.
    pub(super) fn send_version(&mut self, id: PeerId) {
        let Some(peer) = self.peers.get_mut(&id) else {
            return;
        };
        let nonce: u64 = thread_rng().r#gen();
        peer.nonce = nonce;
        let addr = peer.addr;
        let start_height = i32::try_from(self.headers.height()).unwrap_or(i32::MAX);
        let unspecified = SocketAddr::from(([0, 0, 0, 0], 0));

        let version = VersionMessage {
            version: PROTOCOL_VERSION,
            services: OUR_SERVICES,
            timestamp: unix_now(),
            receiver: Address::new(&addr, ServiceFlags::NONE),
            sender: Address::new(&unspecified, OUR_SERVICES),
            nonce,
            user_agent: self.config.user_agent.clone(),
            start_height,
            // We do not process transactions, so ask peers not to announce them.
            relay: false,
        };
        self.send(id, NetworkMessage::Version(version));
    }

    pub(super) async fn on_message(
        &mut self,
        id: PeerId,
        message: NetworkMessage,
    ) -> Result<(), P2pError> {
        let Some(peer) = self.peers.get(&id) else {
            return Ok(());
        };
        if peer.disconnecting {
            return Ok(());
        }

        match (peer.phase, message) {
            (Phase::AwaitingVersion, NetworkMessage::Version(version)) => {
                self.on_version(id, version);
            }
            (Phase::AwaitingVersion, other) => {
                self.misbehaving(id, format!("sent {} before its version", other.cmd()));
            }
            (Phase::AwaitingVerack, NetworkMessage::Verack) => self.on_verack(id),
            (Phase::AwaitingVerack, NetworkMessage::Version(_)) => {
                self.misbehaving(id, "sent a second version message");
            }
            // Feature announcements such as wtxidrelay and sendaddrv2 arrive here.
            (Phase::AwaitingVerack, _) => {}
            (Phase::Ready, message) => self.on_ready_message(id, message).await?,
        }
        Ok(())
    }

    fn on_version(&mut self, id: PeerId, version: VersionMessage) {
        let Some(peer) = self.peers.get_mut(&id) else {
            return;
        };
        if version.nonce == peer.nonce {
            let addr = peer.addr;
            self.disconnect(id, "connected to ourselves");
            self.addrman.mark_failed(addr, unix_now());
            return;
        }
        if version.version < MIN_PEER_VERSION {
            let reason = format!("protocol version {} is too old", version.version);
            self.disconnect(id, reason);
            return;
        }

        let feeler = peer.kind == ConnectionKind::Feeler;
        let serves_blocks = version.services.has(ServiceFlags::NETWORK)
            || version.services.has(ServiceFlags::NETWORK_LIMITED);
        let serves_witness = version.services.has(ServiceFlags::WITNESS);
        if !feeler && (!serves_blocks || !serves_witness) {
            let reason = format!(
                "does not serve witness blocks, services: {}",
                version.services
            );
            self.disconnect(id, reason);
            return;
        }

        peer.version = version.version;
        peer.services = version.services;
        peer.user_agent = version.user_agent;
        peer.start_height = version.start_height;
        peer.phase = Phase::AwaitingVerack;
        self.send(id, NetworkMessage::Verack);
    }

    fn on_verack(&mut self, id: PeerId) {
        let Some(peer) = self.peers.get_mut(&id) else {
            return;
        };
        peer.phase = Phase::Ready;
        let (addr, kind, services, transport) =
            (peer.addr, peer.kind, peer.services, peer.transport);
        let (agent, version, height) = (peer.user_agent.clone(), peer.version, peer.start_height);

        self.addrman
            .mark_good(addr, u64::from(services), unix_now());

        if kind == ConnectionKind::Feeler {
            debug!("peer {id}: feeler to {addr} succeeded");
            self.disconnect(id, "feeler connection");
            return;
        }

        info!(
            "peer {id}: connected to {addr} ({agent}, version {version}, height {height}, {transport}, {services})"
        );

        // Ask for block announcements as headers, and for more addresses to connect to later.
        self.send(id, NetworkMessage::SendHeaders);
        self.send(id, NetworkMessage::GetAddr);

        if self.synced_height.is_none() && self.sync_peer.is_none() {
            self.start_header_sync(id);
        }
        self.request_blocks();
    }

    async fn on_ready_message(
        &mut self,
        id: PeerId,
        message: NetworkMessage,
    ) -> Result<(), P2pError> {
        match message {
            NetworkMessage::Ping(nonce) => self.send(id, NetworkMessage::Pong(nonce)),
            NetworkMessage::Pong(nonce) => self.on_pong(id, nonce),
            NetworkMessage::Addr(addrs) => self.on_addr(id, &addrs),
            NetworkMessage::AddrV2(addrs) => self.on_addr_v2(id, &addrs),
            NetworkMessage::Headers(headers) => self.on_headers(id, headers).await?,
            NetworkMessage::Block(block) => self.on_block(id, block).await?,
            NetworkMessage::Inv(items) => self.on_inv(id, &items),
            NetworkMessage::NotFound(items) => self.on_not_found(id, &items),
            // Requests we do not serve, and features we do not use.
            _ => {}
        }
        Ok(())
    }

    /// Sends keepalive pings and drops peers that stop answering.
    pub(super) fn send_pings(&mut self) {
        let interval = Duration::from_secs(self.config.ping_interval_secs);
        let timeout = Duration::from_secs(self.config.ping_timeout_secs);
        let now = Instant::now();

        let mut to_ping = Vec::new();
        let mut silent = Vec::new();
        for (id, peer) in &self.peers {
            if !peer.is_ready() {
                continue;
            }
            match peer.ping_sent {
                Some((_, sent)) if now.duration_since(sent) > timeout => silent.push(*id),
                Some(_) => {}
                None if now >= peer.next_ping => to_ping.push(*id),
                None => {}
            }
        }

        for id in silent {
            self.disconnect(id, format!("no pong for {} seconds", timeout.as_secs()));
        }
        for id in to_ping {
            let nonce: u64 = thread_rng().r#gen();
            if let Some(peer) = self.peers.get_mut(&id) {
                peer.ping_sent = Some((nonce, now));
                peer.next_ping = now + interval;
            }
            self.send(id, NetworkMessage::Ping(nonce));
        }
    }

    fn on_pong(&mut self, id: PeerId, nonce: u64) {
        let Some(peer) = self.peers.get_mut(&id) else {
            return;
        };
        match peer.ping_sent {
            Some((sent_nonce, sent_at)) if sent_nonce == nonce => {
                peer.latency = Some(Instant::now().duration_since(sent_at));
                peer.ping_sent = None;
            }
            // Bitcoin Core also ignores unsolicited or mismatched pongs.
            _ => debug!("peer {id}: unexpected pong {nonce}"),
        }
    }

    fn on_addr(&mut self, id: PeerId, addrs: &[(u32, Address)]) {
        if self.reject_oversized_addr(id, addrs.len()) {
            return;
        }
        let entries: Vec<(SocketAddr, u64)> = addrs
            .iter()
            .filter_map(|(_, address)| {
                let socket = address.socket_addr().ok()?;
                Some((socket, u64::from(address.services)))
            })
            .collect();
        self.add_addresses(id, &entries);
    }

    fn on_addr_v2(&mut self, id: PeerId, addrs: &[AddrV2Message]) {
        if self.reject_oversized_addr(id, addrs.len()) {
            return;
        }
        let entries: Vec<(SocketAddr, u64)> = addrs
            .iter()
            .filter_map(|entry| {
                // Tor, I2P and CJDNS addresses need a proxy we do not have yet.
                let ip = match entry.addr {
                    AddrV2::Ipv4(ip) => std::net::IpAddr::V4(ip),
                    AddrV2::Ipv6(ip) => std::net::IpAddr::V6(ip),
                    _ => return None,
                };
                Some((SocketAddr::new(ip, entry.port), u64::from(entry.services)))
            })
            .collect();
        self.add_addresses(id, &entries);
    }

    fn reject_oversized_addr(&mut self, id: PeerId, count: usize) -> bool {
        if count > MAX_ADDR_PER_MESSAGE {
            self.misbehaving(
                id,
                format!("sent {count} addresses, limit is {MAX_ADDR_PER_MESSAGE}"),
            );
            return true;
        }
        false
    }

    /// Adds announced addresses, limited by a token bucket per peer as Bitcoin Core does.
    fn add_addresses(&mut self, id: PeerId, entries: &[(SocketAddr, u64)]) {
        let Some(peer) = self.peers.get_mut(&id) else {
            return;
        };
        let now = Instant::now();
        let elapsed = now.duration_since(peer.addr_tokens_updated).as_secs_f64();
        peer.addr_tokens =
            (peer.addr_tokens + elapsed * ADDR_TOKENS_PER_SECOND).min(ADDR_TOKEN_MAX);
        peer.addr_tokens_updated = now;

        let mut allowed = peer.addr_tokens.floor() as usize;
        allowed = allowed.min(entries.len());
        peer.addr_tokens -= allowed as f64;

        let unix = unix_now();
        let added = entries[..allowed]
            .iter()
            .filter(|(addr, services)| self.addrman.add(*addr, *services, unix))
            .count();
        if added > 0 {
            debug!(
                "peer {id}: learned {added} addresses, {} known",
                self.addrman.len()
            );
        }
    }
}

/// The `bitcoin` crate's constant is older than the version we speak; keep them from drifting.
const _: () = assert!(PROTOCOL_VERSION >= CRATE_PROTOCOL_VERSION);

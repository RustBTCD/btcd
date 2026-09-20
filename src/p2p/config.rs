use serde::Deserialize;

use super::transport::TransportPolicy;

/// `[p2p]` section of the config file. Every field is optional.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct P2pConfig {
    /// Peers we always keep connected, as `host:port` or `host`. Like Bitcoin Core's
    /// `-addnode`: they are kept in addition to the automatic connections below.
    pub peers: Vec<String>,
    /// Automatic connections chosen from the address book. Bitcoin Core uses 8.
    pub max_outbound: usize,
    /// Query the chain's DNS seeds when the address book is too small.
    pub use_dns_seeds: bool,
    /// Transport to use. Only the plain v1 transport exists so far.
    pub transport: TransportPolicy,

    /// Seconds allowed for a TCP connection to open.
    pub connect_timeout_secs: u64,
    /// Seconds allowed for the version handshake. Bitcoin Core's `-peertimeout`.
    pub handshake_timeout_secs: u64,
    /// A peer that sends nothing for this many seconds is disconnected.
    pub inactivity_timeout_secs: u64,
    /// Seconds a peer has to answer a header or block request before it is dropped.
    pub request_timeout_secs: u64,
    /// Seconds between pings to each peer. Bitcoin Core uses 2 minutes.
    pub ping_interval_secs: u64,
    /// Seconds to wait for a pong before dropping the peer. Bitcoin Core uses 20 minutes.
    pub ping_timeout_secs: u64,
    /// Seconds between feeler connections, which test untried addresses. Core uses 2 minutes.
    pub feeler_interval_secs: u64,
    /// Minimum seconds between DNS seed lookups.
    pub retry_interval_secs: u64,

    /// Most addresses kept in the address book.
    pub address_book_max: usize,
    /// Default ban length in seconds, for bans added at runtime.
    pub ban_duration_secs: i64,
    /// How long a misbehaving peer stays discouraged, in seconds.
    pub discourage_duration_secs: i64,
    /// Subnets never connected to, as `10.0.0.0/8` or `1.2.3.4`. Active while configured.
    pub bans: Vec<String>,
    /// Permissions by address, as `flag,flag@subnet`, for example `noban@127.0.0.1`.
    /// Only `noban` changes behaviour today; see the permissions module.
    pub whitelist: Vec<String>,

    /// Messages that may be queued towards one peer.
    pub send_queue: usize,
    /// Messages one peer may have queued for the manager before its socket stops being read.
    pub recv_quota: usize,
    /// User agent sent in the version message, in BIP14 format.
    pub user_agent: String,
}

impl Default for P2pConfig {
    fn default() -> Self {
        Self {
            peers: Vec::new(),
            max_outbound: 8,
            use_dns_seeds: true,
            transport: TransportPolicy::V1,

            connect_timeout_secs: 10,
            handshake_timeout_secs: 60,
            inactivity_timeout_secs: 1200,
            request_timeout_secs: 120,
            ping_interval_secs: 120,
            ping_timeout_secs: 1200,
            feeler_interval_secs: 120,
            retry_interval_secs: 30,

            address_book_max: 20_000,
            ban_duration_secs: 86_400,
            discourage_duration_secs: 86_400,
            bans: Vec::new(),
            whitelist: Vec::new(),

            send_queue: 128,
            recv_quota: 16,
            user_agent: concat!("/rust-btcd:", env!("CARGO_PKG_VERSION"), "/").to_string(),
        }
    }
}

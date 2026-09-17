use serde::Deserialize;

/// `[p2p]` section of the config file. Every field is optional.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct P2pConfig {
    /// Peers to connect to, as `host:port`, or `host` for the chain's default port.
    /// When empty, addresses come from the chain's DNS seeds.
    pub peers: Vec<String>,
    /// Number of outbound connections to keep. Bitcoin Core uses 8 full-relay connections.
    pub max_outbound: usize,
    /// Time allowed for a TCP connection to open, in seconds.
    pub connect_timeout_secs: u64,
    /// Time allowed for the version handshake, in seconds. Bitcoin Core `-peertimeout`.
    pub handshake_timeout_secs: u64,
    /// A peer that sends nothing for this long is disconnected, in seconds.
    /// Bitcoin Core uses 20 minutes, and peers normally ping every 2 minutes.
    pub inactivity_timeout_secs: u64,
    /// Time a peer has to answer a `getheaders` or `getdata` request before it is
    /// disconnected as stalling, in seconds.
    pub request_timeout_secs: u64,
    /// Minimum time between two lookups of the peer list or DNS seeds, in seconds.
    pub retry_interval_secs: u64,
    /// User agent sent in the version message, in BIP14 format.
    pub user_agent: String,
}

impl Default for P2pConfig {
    fn default() -> Self {
        Self {
            peers: Vec::new(),
            max_outbound: 8,
            connect_timeout_secs: 10,
            handshake_timeout_secs: 60,
            inactivity_timeout_secs: 1200,
            request_timeout_secs: 120,
            retry_interval_secs: 30,
            user_agent: concat!("/rust-btcd:", env!("CARGO_PKG_VERSION"), "/").to_string(),
        }
    }
}

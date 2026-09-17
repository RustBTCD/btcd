//! Bitcoin P2P networking over the unencrypted v1 transport.
//!
//! One task per peer owns its socket ([`peer`]). A single [`Manager`] owns all peer state, the
//! in-memory header tree and the block download queue. They communicate over channels.
//!
//! Scope for now: outbound connections, header sync, and downloading blocks announced after
//! header sync finished. No serving, no transaction relay, no address gossip, no BIP324.

mod codec;
mod config;
mod manager;
mod peer;

pub use config::P2pConfig;
pub use manager::Manager;

use crate::header_chain::HeaderError;
use crate::storage::StorageError;

#[derive(Debug, thiserror::Error)]
pub enum P2pError {
    #[error("storage error: {0}")]
    Storage(#[from] StorageError),

    #[error("loading headers: {0}")]
    Headers(#[from] HeaderError),

    #[error("background task failed: {0}")]
    Task(#[from] tokio::task::JoinError),

    #[error("no peer addresses: set p2p.peers, or use a chain that has DNS seeds")]
    NoPeers,
}

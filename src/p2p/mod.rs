//! Bitcoin peer-to-peer networking.
//!
//! The layering follows Bitcoin Core:
//!
//! - [`transport`] turns a socket into a stream of messages. Only the plain v1 transport
//!   exists today; BIP324 will be a second implementation behind the same interface.
//! - [`connection`] runs one task per peer and moves messages, like Core's `CConnman` sockets.
//! - [`manager`] owns connection policy and the round-robin message loop.
//! - [`processing`] is the protocol: handshake, keepalive, address relay, like Core's
//!   `net_processing`.
//! - [`sync`] syncs headers and downloads blocks.
//! - [`addrman`], [`banman`] and [`permissions`] hold the address book, bans and per-peer
//!   permissions, backed by the node database.
//!
//! Not implemented yet: inbound connections, serving data, transaction relay, compact blocks,
//! proxies, and block validation.

// The ban and permission APIs are complete ahead of the RPC layer that will drive them.
#![allow(dead_code)]

mod addrman;
mod banman;
mod config;
mod connection;
mod manager;
mod permissions;
mod processing;
mod sync;
mod transport;

pub use config::P2pConfig;
pub use manager::Manager;
pub use permissions::{PermissionError, SubnetError};

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

    #[error("invalid permission setting: {0}")]
    Permissions(#[from] PermissionError),

    #[error("invalid ban setting: {0}")]
    Bans(#[from] SubnetError),

    #[error("no peer addresses: set p2p.peers, or use a chain that has DNS seeds")]
    NoPeers,
}

//! The gossip wire layer.
//!
//! Messages, their payloads and the framing for both transports come from the
//! `bitcoin-p2p-messages` crate. This module adds what a node needs on top: the limits that
//! bound a peer's use of our memory, the reaction each violation calls for, and when a
//! message is valid on a connection.

// The transport and connection layers that consume this are the next step.
#![allow(dead_code, unused_imports)]

pub mod limits;
pub mod serialize;
pub mod validate;
pub mod window;

/// Every message of the peer-to-peer protocol.
pub use p2p::message::NetworkMessage as Message;
pub use serialize::{block_without_witness, transaction_without_witness};
pub use validate::{Overflow, WireError, check, requests_unoffered_service};
pub use window::Window;

/// What a peer does about a violation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reaction {
    /// Drop the message and continue.
    Discard,
    /// Close the connection.
    Terminate,
    /// Close the connection and avoid the address for a bounded period, because the peer made
    /// us spend memory or bandwidth it had no reason to.
    TerminateAndRecord,
}

//! Transport abstraction: turns a socket into a stream of messages.
//!
//! Everything above this layer sees only [`NetworkMessage`], so a transport is free to choose
//! its own framing, encryption and command encoding. Today there is one implementation, the
//! plain v1 framing. The v2 encrypted transport of BIP324 will be added as a second one.
//!
//! Connecting lives here rather than in the connection task because negotiating v2 may require
//! reconnecting and retrying with v1.

mod v1;

use std::future::Future;
use std::net::SocketAddr;
use std::time::Duration;

use bitcoin::Network;
use bitcoin::consensus::encode;
use bitcoin::p2p::Magic;
use bitcoin::p2p::message::NetworkMessage;
use serde::Deserialize;
use tokio::time::timeout;

pub use v1::V1Transport;

#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    #[error("connection timed out")]
    Timeout,

    #[error("connection error: {0}")]
    Io(#[from] std::io::Error),

    #[error("wrong network magic {found}, expected {expected}")]
    WrongMagic { found: Magic, expected: Magic },

    #[error("message of {len} bytes exceeds the {limit} byte limit")]
    TooLarge { len: usize, limit: usize },

    #[error("invalid message: {0}")]
    Decode(#[from] encode::Error),
}

/// Which transport a connection ended up using.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportKind {
    V1,
    // V2 (BIP324) goes here.
}

impl std::fmt::Display for TransportKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransportKind::V1 => f.write_str("v1"),
        }
    }
}

/// Which transports to use when connecting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TransportPolicy {
    /// Plain, unencrypted transport only.
    #[default]
    V1,
    // V2Preferred: try BIP324 first and fall back to v1, once v2 exists.
}

/// Reads whole messages from a peer.
pub trait MessageReader: Send + 'static {
    fn read(&mut self) -> impl Future<Output = Result<NetworkMessage, TransportError>> + Send;
}

/// Writes whole messages to a peer.
pub trait MessageWriter: Send + 'static {
    fn write(
        &mut self,
        message: NetworkMessage,
    ) -> impl Future<Output = Result<(), TransportError>> + Send;
}

/// A connected transport, before it is split into its two halves.
pub trait Transport: Send + 'static {
    type Reader: MessageReader;
    type Writer: MessageWriter;

    fn kind(&self) -> TransportKind;
    fn split(self) -> (Self::Reader, Self::Writer);
}

/// A connected transport of any kind. The connection task matches on this once and then works
/// with a concrete type, so there is no dynamic dispatch on the hot path.
pub enum AnyTransport {
    V1(V1Transport),
}

impl AnyTransport {
    pub fn kind(&self) -> TransportKind {
        match self {
            AnyTransport::V1(transport) => transport.kind(),
        }
    }
}

/// Opens a connection and negotiates a transport.
pub async fn connect(
    addr: SocketAddr,
    network: Network,
    policy: TransportPolicy,
    connect_timeout: Duration,
) -> Result<AnyTransport, TransportError> {
    match policy {
        // With v2 added, this arm tries BIP324 first and falls back to v1 on failure.
        TransportPolicy::V1 => {
            let stream = timeout(connect_timeout, tokio::net::TcpStream::connect(addr))
                .await
                .map_err(|_| TransportError::Timeout)??;
            let _ = stream.set_nodelay(true);
            Ok(AnyTransport::V1(V1Transport::new(stream, network.magic())))
        }
    }
}

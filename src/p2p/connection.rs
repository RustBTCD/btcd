//! One task per peer, owning its transport.
//!
//! This layer knows nothing about the Bitcoin protocol: it connects, reads messages, writes
//! what the manager asks it to, and reports when the connection ends. Bitcoin Core splits the
//! same way, with `CConnman` moving bytes and `net_processing` interpreting them.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use bitcoin::Network;
use bitcoin::p2p::message::NetworkMessage;
use tokio::sync::{OwnedSemaphorePermit, Semaphore, mpsc};
use tokio::time::timeout;

use super::transport::{
    AnyTransport, MessageReader, MessageWriter, Transport, TransportError, TransportKind,
    TransportPolicy, connect,
};

pub type PeerId = u64;

pub struct ConnectionConfig {
    pub network: Network,
    pub policy: TransportPolicy,
    pub connect_timeout: Duration,
    pub inactivity_timeout: Duration,
    /// Messages the manager may have queued for one peer before writes start waiting.
    pub send_queue: usize,
    /// Messages one peer may have waiting for the manager before we stop reading its socket.
    pub recv_quota: usize,
}

#[derive(Debug)]
pub enum ConnectionCommand {
    Send(NetworkMessage),
    Disconnect(String),
}

#[derive(Debug)]
pub enum ConnectionEvent {
    Connected {
        id: PeerId,
        kind: TransportKind,
        commands: mpsc::Sender<ConnectionCommand>,
    },
    Message {
        id: PeerId,
        message: NetworkMessage,
        /// Held until the manager has handled the message, which bounds each peer's queue.
        permit: OwnedSemaphorePermit,
    },
    /// Sent exactly once per spawned connection, including when it never opened.
    Closed { id: PeerId, reason: String },
}

#[derive(Debug, thiserror::Error)]
pub enum ConnectionError {
    #[error("{0}")]
    Transport(#[from] TransportError),

    #[error("no message for {0} seconds")]
    Inactive(u64),

    #[error("disconnected by us: {0}")]
    Disconnected(String),
}

/// Starts a connection task. It always ends by sending [`ConnectionEvent::Closed`].
pub fn spawn(
    id: PeerId,
    addr: SocketAddr,
    config: Arc<ConnectionConfig>,
    events: mpsc::Sender<ConnectionEvent>,
) {
    tokio::spawn(async move {
        let reason = match run(id, addr, &config, &events).await {
            Ok(()) => "connection closed".to_string(),
            Err(err) => err.to_string(),
        };
        let _ = events.send(ConnectionEvent::Closed { id, reason }).await;
    });
}

async fn run(
    id: PeerId,
    addr: SocketAddr,
    config: &ConnectionConfig,
    events: &mpsc::Sender<ConnectionEvent>,
) -> Result<(), ConnectionError> {
    let transport = connect(addr, config.network, config.policy, config.connect_timeout).await?;
    let kind = transport.kind();
    let (commands_tx, commands_rx) = mpsc::channel(config.send_queue);

    let connected = ConnectionEvent::Connected {
        id,
        kind,
        commands: commands_tx,
    };
    if events.send(connected).await.is_err() {
        return Ok(());
    }

    // One match on the transport kind, then concrete types all the way down.
    match transport {
        AnyTransport::V1(transport) => {
            let (reader, writer) = transport.split();
            serve(id, reader, writer, commands_rx, config, events).await
        }
    }
}

async fn serve<R, W>(
    id: PeerId,
    reader: R,
    writer: W,
    commands: mpsc::Receiver<ConnectionCommand>,
    config: &ConnectionConfig,
    events: &mpsc::Sender<ConnectionEvent>,
) -> Result<(), ConnectionError>
where
    R: MessageReader,
    W: MessageWriter,
{
    let quota = Arc::new(Semaphore::new(config.recv_quota));
    // Reading and writing run concurrently; whichever ends first ends the connection.
    tokio::select! {
        result = read_loop(id, reader, quota, config, events) => result,
        result = write_loop(writer, commands) => result,
    }
}

async fn read_loop<R: MessageReader>(
    id: PeerId,
    mut reader: R,
    quota: Arc<Semaphore>,
    config: &ConnectionConfig,
    events: &mpsc::Sender<ConnectionEvent>,
) -> Result<(), ConnectionError> {
    loop {
        // Taking the permit before reading means a peer at its quota stops being read from,
        // which pushes back through TCP instead of filling our memory.
        let Ok(permit) = quota.clone().acquire_owned().await else {
            return Ok(());
        };
        let message = timeout(config.inactivity_timeout, reader.read())
            .await
            .map_err(|_| ConnectionError::Inactive(config.inactivity_timeout.as_secs()))??;

        if events
            .send(ConnectionEvent::Message {
                id,
                message,
                permit,
            })
            .await
            .is_err()
        {
            return Ok(());
        }
    }
}

async fn write_loop<W: MessageWriter>(
    mut writer: W,
    mut commands: mpsc::Receiver<ConnectionCommand>,
) -> Result<(), ConnectionError> {
    while let Some(command) = commands.recv().await {
        match command {
            ConnectionCommand::Send(message) => writer.write(message).await?,
            ConnectionCommand::Disconnect(reason) => {
                return Err(ConnectionError::Disconnected(reason));
            }
        }
    }
    Ok(())
}

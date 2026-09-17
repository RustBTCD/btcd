//! One task per outbound peer: connect, handshake, then pass messages between the socket and
//! the manager until either side ends the connection.

use std::hash::{BuildHasher, RandomState};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bitcoin::p2p::address::Address;
use bitcoin::p2p::message::NetworkMessage;
use bitcoin::p2p::message_network::VersionMessage;
use bitcoin::p2p::{Magic, ServiceFlags};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::time::timeout;

use super::codec::{CodecError, read_message, write_message};
use super::config::P2pConfig;
use crate::chain_params::Chain;

pub type PeerId = u64;

/// Protocol version we speak. 70016 adds `wtxidrelay` (BIP339), which we accept but don't use.
const PROTOCOL_VERSION: u32 = 70016;
/// Oldest peer version we accept. 70012 adds `sendheaders` (BIP130), which we rely on.
const MIN_PEER_VERSION: u32 = 70012;
/// We understand witness data but serve nothing.
const OUR_SERVICES: ServiceFlags = ServiceFlags::WITNESS;

/// Settings shared by all peer tasks.
pub struct PeerContext {
    pub magic: Magic,
    pub user_agent: String,
    pub connect_timeout: Duration,
    pub handshake_timeout: Duration,
    pub inactivity_timeout: Duration,
}

impl PeerContext {
    pub fn new(chain: Chain, config: &P2pConfig) -> Self {
        Self {
            magic: chain.network().magic(),
            user_agent: config.user_agent.clone(),
            connect_timeout: Duration::from_secs(config.connect_timeout_secs),
            handshake_timeout: Duration::from_secs(config.handshake_timeout_secs),
            inactivity_timeout: Duration::from_secs(config.inactivity_timeout_secs),
        }
    }
}

/// What the peer told us in its version message.
#[derive(Debug, Clone)]
pub struct PeerInfo {
    pub addr: SocketAddr,
    pub version: u32,
    pub services: ServiceFlags,
    pub user_agent: String,
    pub start_height: i32,
}

/// Manager to peer.
#[derive(Debug)]
pub enum PeerCommand {
    Send(NetworkMessage),
    Disconnect(String),
}

/// Peer to manager.
#[derive(Debug)]
pub enum PeerEvent {
    Connected {
        id: PeerId,
        info: PeerInfo,
        commands: mpsc::UnboundedSender<PeerCommand>,
    },
    Message {
        id: PeerId,
        message: NetworkMessage,
    },
    /// Sent exactly once per spawned peer, also when the connection never opened.
    Disconnected {
        id: PeerId,
        reason: String,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum PeerError {
    #[error("connection timed out")]
    ConnectTimeout,

    #[error("connection failed: {0}")]
    Connect(std::io::Error),

    #[error("handshake timed out")]
    HandshakeTimeout,

    #[error("{0}")]
    Codec(#[from] CodecError),

    #[error("protocol violation: {0}")]
    Protocol(String),

    #[error("connected to ourselves")]
    SelfConnection,

    #[error("peer protocol version {0} is older than {MIN_PEER_VERSION}")]
    ObsoleteVersion(u32),

    #[error("peer does not serve witness blocks, services: {0}")]
    MissingServices(ServiceFlags),

    #[error("no message for {0} seconds")]
    Inactive(u64),

    #[error("disconnected by us: {0}")]
    Disconnected(String),
}

/// Starts a peer task. It always ends by sending [`PeerEvent::Disconnected`].
pub fn spawn(
    id: PeerId,
    addr: SocketAddr,
    start_height: u32,
    ctx: Arc<PeerContext>,
    events: mpsc::Sender<PeerEvent>,
) {
    tokio::spawn(async move {
        let reason = match run(id, addr, start_height, &ctx, &events).await {
            Ok(()) => "connection closed".to_string(),
            Err(err) => err.to_string(),
        };
        let _ = events.send(PeerEvent::Disconnected { id, reason }).await;
    });
}

async fn run(
    id: PeerId,
    addr: SocketAddr,
    start_height: u32,
    ctx: &PeerContext,
    events: &mpsc::Sender<PeerEvent>,
) -> Result<(), PeerError> {
    let stream = timeout(ctx.connect_timeout, TcpStream::connect(addr))
        .await
        .map_err(|_| PeerError::ConnectTimeout)?
        .map_err(PeerError::Connect)?;
    let _ = stream.set_nodelay(true);
    let (mut reader, mut writer) = stream.into_split();

    let handshake = handshake(&mut reader, &mut writer, addr, start_height, ctx);
    let info = timeout(ctx.handshake_timeout, handshake)
        .await
        .map_err(|_| PeerError::HandshakeTimeout)??;

    // Ask for new blocks to be announced with `headers` instead of `inv`.
    write_message(&mut writer, ctx.magic, NetworkMessage::SendHeaders).await?;

    let (commands_tx, commands_rx) = mpsc::unbounded_channel();
    let connected = PeerEvent::Connected {
        id,
        info,
        commands: commands_tx.clone(),
    };
    if events.send(connected).await.is_err() {
        return Ok(());
    }

    // Reading and writing run concurrently; whichever ends first ends the connection.
    tokio::select! {
        result = read_loop(id, &mut reader, ctx, &commands_tx, events) => result,
        result = write_loop(&mut writer, ctx.magic, commands_rx) => result,
    }
}

/// Version handshake for an outbound connection.
///
/// We send `version`, wait for theirs, send `verack`, then wait for their `verack`. Feature
/// messages a peer sends between its `version` and `verack` are accepted and ignored.
async fn handshake<R, W>(
    reader: &mut R,
    writer: &mut W,
    addr: SocketAddr,
    start_height: u32,
    ctx: &PeerContext,
) -> Result<PeerInfo, PeerError>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let nonce = random_nonce();
    let unspecified = SocketAddr::from(([0, 0, 0, 0], 0));
    let ours = VersionMessage {
        version: PROTOCOL_VERSION,
        services: OUR_SERVICES,
        timestamp: unix_time(),
        receiver: Address::new(&addr, ServiceFlags::NONE),
        sender: Address::new(&unspecified, OUR_SERVICES),
        nonce,
        user_agent: ctx.user_agent.clone(),
        start_height: i32::try_from(start_height).unwrap_or(i32::MAX),
        // We don't process transactions, so ask peers not to announce them.
        relay: false,
    };
    write_message(writer, ctx.magic, NetworkMessage::Version(ours)).await?;

    let theirs = match read_message(reader, ctx.magic).await? {
        NetworkMessage::Version(version) => version,
        other => {
            return Err(PeerError::Protocol(format!(
                "expected version, got {}",
                other.cmd()
            )));
        }
    };
    if theirs.nonce == nonce {
        return Err(PeerError::SelfConnection);
    }
    if theirs.version < MIN_PEER_VERSION {
        return Err(PeerError::ObsoleteVersion(theirs.version));
    }
    let serves_blocks = theirs.services.has(ServiceFlags::NETWORK)
        || theirs.services.has(ServiceFlags::NETWORK_LIMITED);
    if !serves_blocks || !theirs.services.has(ServiceFlags::WITNESS) {
        return Err(PeerError::MissingServices(theirs.services));
    }

    write_message(writer, ctx.magic, NetworkMessage::Verack).await?;
    loop {
        match read_message(reader, ctx.magic).await? {
            NetworkMessage::Verack => break,
            NetworkMessage::Version(_) => {
                return Err(PeerError::Protocol("duplicate version".to_string()));
            }
            // wtxidrelay, sendaddrv2 and similar feature announcements.
            _ => continue,
        }
    }

    Ok(PeerInfo {
        addr,
        version: theirs.version,
        services: theirs.services,
        user_agent: theirs.user_agent,
        start_height: theirs.start_height,
    })
}

async fn read_loop<R>(
    id: PeerId,
    reader: &mut R,
    ctx: &PeerContext,
    commands: &mpsc::UnboundedSender<PeerCommand>,
    events: &mpsc::Sender<PeerEvent>,
) -> Result<(), PeerError>
where
    R: AsyncRead + Unpin,
{
    loop {
        let message = timeout(ctx.inactivity_timeout, read_message(reader, ctx.magic))
            .await
            .map_err(|_| PeerError::Inactive(ctx.inactivity_timeout.as_secs()))??;

        match message {
            NetworkMessage::Ping(nonce) => {
                let _ = commands.send(PeerCommand::Send(NetworkMessage::Pong(nonce)));
            }
            message if is_forwarded(&message) => {
                let event = PeerEvent::Message { id, message };
                if events.send(event).await.is_err() {
                    return Ok(());
                }
            }
            // Everything else is not handled yet.
            _ => {}
        }
    }
}

async fn write_loop<W>(
    writer: &mut W,
    magic: Magic,
    mut commands: mpsc::UnboundedReceiver<PeerCommand>,
) -> Result<(), PeerError>
where
    W: AsyncWrite + Unpin,
{
    while let Some(command) = commands.recv().await {
        match command {
            PeerCommand::Send(message) => write_message(writer, magic, message).await?,
            PeerCommand::Disconnect(reason) => return Err(PeerError::Disconnected(reason)),
        }
    }
    Ok(())
}

/// Messages the manager acts on.
fn is_forwarded(message: &NetworkMessage) -> bool {
    matches!(
        message,
        NetworkMessage::Headers(_)
            | NetworkMessage::Inv(_)
            | NetworkMessage::Block(_)
            | NetworkMessage::NotFound(_)
    )
}

/// Detects connecting to ourselves; it does not need to be unpredictable.
fn random_nonce() -> u64 {
    RandomState::new().hash_one(SystemTime::now())
}

fn unix_time() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{DuplexStream, ReadHalf, WriteHalf, duplex, split};

    const MAGIC: Magic = Magic::REGTEST;

    fn ctx() -> PeerContext {
        PeerContext::new(Chain::Regtest, &P2pConfig::default())
    }

    fn addr() -> SocketAddr {
        SocketAddr::from(([127, 0, 0, 1], 18444))
    }

    type Half = (ReadHalf<DuplexStream>, WriteHalf<DuplexStream>);

    fn pipe() -> (Half, Half) {
        let (ours, theirs) = duplex(64 * 1024);
        (split(ours), split(theirs))
    }

    /// A remote peer that answers our version with `reply`, then sends `after`.
    async fn remote(
        (mut reader, mut writer): Half,
        reply: impl FnOnce(&VersionMessage) -> NetworkMessage,
        after: Vec<NetworkMessage>,
    ) -> Vec<NetworkMessage> {
        let mut received = Vec::new();
        let ours = match read_message(&mut reader, MAGIC).await.unwrap() {
            NetworkMessage::Version(v) => v,
            other => panic!("expected version, got {other:?}"),
        };
        write_message(&mut writer, MAGIC, reply(&ours))
            .await
            .unwrap();
        for message in after {
            write_message(&mut writer, MAGIC, message).await.unwrap();
        }
        received.push(NetworkMessage::Version(ours));
        if let Ok(message) = read_message(&mut reader, MAGIC).await {
            received.push(message);
        }
        received
    }

    fn version(nonce: u64, version: u32, services: ServiceFlags) -> NetworkMessage {
        NetworkMessage::Version(VersionMessage {
            version,
            services,
            timestamp: unix_time(),
            receiver: Address::new(&addr(), ServiceFlags::NONE),
            sender: Address::new(&addr(), services),
            nonce,
            user_agent: "/Satoshi:31.0.0/".to_string(),
            start_height: 7,
            relay: true,
        })
    }

    fn full_node() -> ServiceFlags {
        ServiceFlags::NETWORK | ServiceFlags::WITNESS
    }

    #[tokio::test]
    async fn handshake_succeeds() {
        let ((mut r, mut w), remote_half) = pipe();
        let peer = tokio::spawn(remote(
            remote_half,
            |ours| version(ours.nonce + 1, 70016, full_node()),
            vec![
                NetworkMessage::WtxidRelay,
                NetworkMessage::SendAddrV2,
                NetworkMessage::Verack,
            ],
        ));

        let info = handshake(&mut r, &mut w, addr(), 5, &ctx()).await.unwrap();
        assert_eq!(info.user_agent, "/Satoshi:31.0.0/");
        assert_eq!(info.start_height, 7);

        let received = peer.await.unwrap();
        match &received[0] {
            NetworkMessage::Version(v) => {
                assert_eq!(v.version, PROTOCOL_VERSION);
                assert_eq!(v.start_height, 5);
                assert!(!v.relay);
            }
            other => panic!("expected version, got {other:?}"),
        }
        assert_eq!(received[1], NetworkMessage::Verack);
    }

    #[tokio::test]
    async fn rejects_connecting_to_ourselves() {
        let ((mut r, mut w), remote_half) = pipe();
        tokio::spawn(remote(
            remote_half,
            |ours| version(ours.nonce, 70016, full_node()),
            vec![],
        ));
        let err = handshake(&mut r, &mut w, addr(), 0, &ctx()).await;
        assert!(matches!(err, Err(PeerError::SelfConnection)));
    }

    #[tokio::test]
    async fn rejects_peers_without_witness_blocks() {
        let ((mut r, mut w), remote_half) = pipe();
        tokio::spawn(remote(
            remote_half,
            |ours| version(ours.nonce + 1, 70016, ServiceFlags::NETWORK),
            vec![],
        ));
        let err = handshake(&mut r, &mut w, addr(), 0, &ctx()).await;
        assert!(matches!(err, Err(PeerError::MissingServices(_))));
    }

    #[tokio::test]
    async fn rejects_old_protocol_versions() {
        let ((mut r, mut w), remote_half) = pipe();
        tokio::spawn(remote(
            remote_half,
            |ours| version(ours.nonce + 1, 70011, full_node()),
            vec![],
        ));
        let err = handshake(&mut r, &mut w, addr(), 0, &ctx()).await;
        assert!(matches!(err, Err(PeerError::ObsoleteVersion(70011))));
    }

    #[tokio::test]
    async fn rejects_messages_before_version() {
        let ((mut r, mut w), remote_half) = pipe();
        tokio::spawn(remote(remote_half, |_| NetworkMessage::Verack, vec![]));
        let err = handshake(&mut r, &mut w, addr(), 0, &ctx()).await;
        assert!(matches!(err, Err(PeerError::Protocol(_))));
    }
}

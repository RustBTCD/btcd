//! Framing for the v1 transport: a 24-byte header (magic, command, length, checksum), then
//! the payload. The `bitcoin` crate encodes and verifies the frame; this module moves the
//! bytes and enforces the size limit before allocating.

use bitcoin::consensus::encode::{self, deserialize, serialize};
use bitcoin::p2p::Magic;
use bitcoin::p2p::message::{NetworkMessage, RawNetworkMessage};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// Bitcoin Core `MAX_PROTOCOL_MESSAGE_LENGTH`.
pub const MAX_PAYLOAD_LEN: usize = 4_000_000;
const HEADER_LEN: usize = 24;
const LENGTH_FIELD: std::ops::Range<usize> = 16..20;

#[derive(Debug, thiserror::Error)]
pub enum CodecError {
    #[error("connection error: {0}")]
    Io(#[from] std::io::Error),

    #[error("wrong network magic {found}, expected {expected}")]
    WrongMagic { found: Magic, expected: Magic },

    #[error("message of {len} bytes exceeds the {MAX_PAYLOAD_LEN} byte limit")]
    TooLarge { len: usize },

    #[error("invalid message: {0}")]
    Decode(#[from] encode::Error),
}

pub async fn read_message<R>(reader: &mut R, magic: Magic) -> Result<NetworkMessage, CodecError>
where
    R: AsyncRead + Unpin,
{
    let mut header = [0u8; HEADER_LEN];
    reader.read_exact(&mut header).await?;

    let found = Magic::from_bytes(header[..4].try_into().expect("slice is 4 bytes"));
    if found != magic {
        return Err(CodecError::WrongMagic {
            found,
            expected: magic,
        });
    }
    let len = u32::from_le_bytes(header[LENGTH_FIELD].try_into().expect("slice is 4 bytes"));
    let len = len as usize;
    if len > MAX_PAYLOAD_LEN {
        return Err(CodecError::TooLarge { len });
    }

    let mut frame = vec![0u8; HEADER_LEN + len];
    frame[..HEADER_LEN].copy_from_slice(&header);
    reader.read_exact(&mut frame[HEADER_LEN..]).await?;

    // Decoding checks the checksum and that the payload matches the command.
    let raw: RawNetworkMessage = deserialize(&frame)?;
    Ok(raw.into_payload())
}

pub async fn write_message<W>(
    writer: &mut W,
    magic: Magic,
    message: NetworkMessage,
) -> Result<(), CodecError>
where
    W: AsyncWrite + Unpin,
{
    let frame = serialize(&RawNetworkMessage::new(magic, message));
    writer.write_all(&frame).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bitcoin::Network;

    const MAGIC: Magic = Magic::REGTEST;

    async fn frame(message: NetworkMessage) -> Vec<u8> {
        let mut bytes = Vec::new();
        write_message(&mut bytes, MAGIC, message).await.unwrap();
        bytes
    }

    #[tokio::test]
    async fn roundtrip() {
        let bytes = frame(NetworkMessage::Ping(42)).await;
        assert_eq!(bytes.len(), HEADER_LEN + 8);
        let message = read_message(&mut bytes.as_slice(), MAGIC).await.unwrap();
        assert_eq!(message, NetworkMessage::Ping(42));
    }

    #[tokio::test]
    async fn reads_consecutive_messages() {
        let mut bytes = frame(NetworkMessage::Verack).await;
        bytes.extend(frame(NetworkMessage::SendHeaders).await);
        let mut reader = bytes.as_slice();
        assert_eq!(
            read_message(&mut reader, MAGIC).await.unwrap(),
            NetworkMessage::Verack
        );
        assert_eq!(
            read_message(&mut reader, MAGIC).await.unwrap(),
            NetworkMessage::SendHeaders
        );
    }

    #[tokio::test]
    async fn rejects_wrong_magic() {
        let bytes = frame(NetworkMessage::Verack).await;
        let err = read_message(&mut bytes.as_slice(), Network::Bitcoin.magic()).await;
        assert!(matches!(err, Err(CodecError::WrongMagic { .. })));
    }

    #[tokio::test]
    async fn rejects_oversized_length_before_reading_payload() {
        let mut bytes = frame(NetworkMessage::Verack).await;
        bytes[LENGTH_FIELD].copy_from_slice(&(MAX_PAYLOAD_LEN as u32 + 1).to_le_bytes());
        let err = read_message(&mut bytes.as_slice(), MAGIC).await;
        assert!(matches!(err, Err(CodecError::TooLarge { .. })));
    }

    #[tokio::test]
    async fn rejects_bad_checksum() {
        let mut bytes = frame(NetworkMessage::Ping(42)).await;
        let last = bytes.len() - 1;
        bytes[last] ^= 1;
        let err = read_message(&mut bytes.as_slice(), MAGIC).await;
        assert!(matches!(err, Err(CodecError::Decode(_))));
    }

    #[tokio::test]
    async fn truncated_frame_is_an_io_error() {
        let bytes = frame(NetworkMessage::Ping(42)).await;
        let err = read_message(&mut &bytes[..bytes.len() - 1], MAGIC).await;
        assert!(matches!(err, Err(CodecError::Io(_))));
    }
}

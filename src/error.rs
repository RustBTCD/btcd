//! Top-level error for the node binary.
//!
//! Each module owns its own error type. This enum only lets `?` combine them in `main`.
//! Variants are transparent: they add no text, so the message is the module error's message.

use crate::config::ConfigError;
use crate::p2p::P2pError;
use crate::storage::StorageError;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Config(#[from] ConfigError),

    #[error(transparent)]
    Storage(#[from] StorageError),

    #[error(transparent)]
    P2p(#[from] P2pError),
}

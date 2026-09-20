//! Consensus validation hooks.
//!
//! These are placeholders so the networking code already calls validation at the right points.
//! The real rules land with the consensus module; see `docs/PLAN2.md`, stage 1 steps 1.1.2
//! and 1.1.3, for the list.

// Variants exist for the rules that land with the consensus module.
#![allow(dead_code)]

use bitcoin::Block;
use bitcoin::block::Header;

use crate::storage::HeaderEntry;

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum ValidationError {
    /// Placeholder so the enum has a variant and callers can already match on errors.
    #[error("invalid header: {0}")]
    Header(String),

    #[error("invalid block: {0}")]
    Block(String),
}

/// Contextual header checks against the parent entry.
///
/// TODO: implement the difficulty schedule, median time past, the future time limit, and
/// version rules. Until then, only the cheap checks in `HeaderChain` apply: the header must
/// connect, meet the target it claims, and not claim a target easier than the chain allows.
pub fn check_header(_header: &Header, _parent: &HeaderEntry) -> Result<(), ValidationError> {
    Ok(())
}

/// Full block validation against the chain state.
///
/// TODO: implement the stateless checks, then the contextual ones and the connect step that
/// updates the UTXO set. The storage layer already has connect and disconnect with undo data.
pub fn check_block(_block: &Block, _entry: &HeaderEntry) -> Result<(), ValidationError> {
    Ok(())
}

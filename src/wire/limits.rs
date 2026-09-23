//! Bounds on how much one message may make us hold in memory.
//!
//! Limits the `bitcoin-p2p-messages` crate already applies while decoding, such as the
//! 256-byte user agent and the sizes inside a feature message, are not repeated here.

/// Largest payload of a plaintext frame. The message crate allows more.
pub const MAX_PAYLOAD: usize = 4_000_000;
/// Address records in one `addr` or `addrv2` message.
pub const MAX_ADDR_RECORDS: usize = 1000;
/// Inventory vectors in one `inv`, `getdata` or `notfound` message.
pub const MAX_INV_VECTORS: usize = 50_000;
/// Headers in one `headers` message.
pub const MAX_HEADERS: usize = 2000;
/// Hashes in a block locator, matching Bitcoin Core's `MAX_LOCATOR_SZ`.
pub const MAX_LOCATOR_HASHES: usize = 101;

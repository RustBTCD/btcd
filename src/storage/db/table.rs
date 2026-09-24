//! What a table is, and the codecs its keys and values use.

use crate::storage::Result;

/// How a table's data is used. An engine may turn this into storage settings, or ignore it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Profile {
    /// Small values, read in ranges as often as by key.
    Sequential,
    /// Small values looked up by key, often for keys that are absent.
    PointLookup,
    /// Large values, where copying them around is the dominant cost.
    LargeValues,
}

/// A key, value or key prefix that can be written as bytes.
pub trait Encode {
    fn encode(&self) -> Vec<u8>;
}

/// A value that can be read back from bytes.
pub trait Decode: Sized {
    fn decode(bytes: &[u8]) -> Result<Self>;
}

/// One table: its name, how it is used, and the types it stores.
pub trait Table {
    const NAME: &'static str;
    const PROFILE: Profile;

    type Key: Encode;
    type Value: Encode + Decode;
    /// The leading part of a key a scan may select on. Tables that are never scanned by
    /// prefix use `()`, which selects the whole table.
    type Prefix: Encode;
}

/// An empty prefix matches every key, so `()` scans a whole table.
impl Encode for () {
    fn encode(&self) -> Vec<u8> {
        Vec::new()
    }
}

/// Heights are big-endian so that a scan walks the chain in order.
impl Encode for u32 {
    fn encode(&self) -> Vec<u8> {
        self.to_be_bytes().to_vec()
    }
}

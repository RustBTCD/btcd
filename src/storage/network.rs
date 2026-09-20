//! Persistent peer addresses and bans.
//!
//! The address book is Bitcoin Core's `peers.dat`, and the ban list its `banlist.json`, both
//! kept in the node database instead of separate files.

use std::net::{IpAddr, Ipv6Addr, SocketAddr};
use std::sync::Arc;

use rocksdb::{DB, IteratorMode, WriteBatch};

use super::{Result, StorageError, cf};

/// Key length of an address: 16-byte IPv6 form plus a 2-byte port.
const ADDRESS_KEY_LEN: usize = 18;
const ADDRESS_VALUE_LEN: usize = 8 + 8 + 8 + 8 + 4 + 1;

/// What we know about one peer address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddressRecord {
    pub addr: SocketAddr,
    /// Services the peer claimed, as a raw bit mask.
    pub services: u64,
    /// Unix time the address was last announced to us.
    pub last_seen: i64,
    /// Unix time we last tried to connect.
    pub last_try: i64,
    /// Unix time a connection last succeeded.
    pub last_success: i64,
    /// Failed attempts since the last success.
    pub attempts: u32,
    /// True once a connection to this address has succeeded: Bitcoin Core's "tried" table.
    pub tried: bool,
}

impl AddressRecord {
    pub fn new(addr: SocketAddr, services: u64, now: i64) -> Self {
        Self {
            addr,
            services,
            last_seen: now,
            last_try: 0,
            last_success: 0,
            attempts: 0,
            tried: false,
        }
    }

    pub fn key(&self) -> [u8; ADDRESS_KEY_LEN] {
        address_key(self.addr)
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(ADDRESS_VALUE_LEN);
        bytes.extend_from_slice(&self.services.to_le_bytes());
        bytes.extend_from_slice(&self.last_seen.to_le_bytes());
        bytes.extend_from_slice(&self.last_try.to_le_bytes());
        bytes.extend_from_slice(&self.last_success.to_le_bytes());
        bytes.extend_from_slice(&self.attempts.to_le_bytes());
        bytes.push(u8::from(self.tried));
        bytes
    }

    fn from_bytes(key: &[u8], bytes: &[u8]) -> Result<Self> {
        let corrupted = |reason: &'static str| StorageError::corrupted("address record", reason);
        if bytes.len() != ADDRESS_VALUE_LEN {
            return Err(corrupted("wrong length"));
        }
        let addr = address_from_key(key).ok_or_else(|| corrupted("invalid key"))?;
        let number = |range: std::ops::Range<usize>| -> [u8; 8] {
            bytes[range].try_into().expect("slice is 8 bytes")
        };
        Ok(Self {
            addr,
            services: u64::from_le_bytes(number(0..8)),
            last_seen: i64::from_le_bytes(number(8..16)),
            last_try: i64::from_le_bytes(number(16..24)),
            last_success: i64::from_le_bytes(number(24..32)),
            attempts: u32::from_le_bytes(bytes[32..36].try_into().expect("slice is 4 bytes")),
            tried: bytes[36] == 1,
        })
    }
}

pub fn address_key(addr: SocketAddr) -> [u8; ADDRESS_KEY_LEN] {
    let ip = match addr.ip() {
        IpAddr::V4(ipv4) => ipv4.to_ipv6_mapped(),
        IpAddr::V6(ipv6) => ipv6,
    };
    let mut key = [0u8; ADDRESS_KEY_LEN];
    key[..16].copy_from_slice(&ip.octets());
    key[16..].copy_from_slice(&addr.port().to_be_bytes());
    key
}

fn address_from_key(key: &[u8]) -> Option<SocketAddr> {
    let ip: [u8; 16] = key.get(..16)?.try_into().ok()?;
    let port: [u8; 2] = key.get(16..18)?.try_into().ok()?;
    let ip = Ipv6Addr::from(ip);
    let ip = match ip.to_ipv4_mapped() {
        Some(ipv4) => IpAddr::V4(ipv4),
        None => IpAddr::V6(ip),
    };
    Some(SocketAddr::new(ip, u16::from_be_bytes(port)))
}

/// The address book.
#[derive(Clone)]
pub struct AddressStore {
    db: Arc<DB>,
}

impl AddressStore {
    pub(crate) fn new(db: Arc<DB>) -> Self {
        Self { db }
    }

    pub fn put_many(&self, records: &[AddressRecord]) -> Result<()> {
        let cf = cf(&self.db, cf::PEERS)?;
        let mut batch = WriteBatch::default();
        for record in records {
            batch.put_cf(cf, record.key(), record.to_bytes());
        }
        self.db.write(batch)?;
        Ok(())
    }

    pub fn delete_many(&self, addrs: &[SocketAddr]) -> Result<()> {
        let cf = cf(&self.db, cf::PEERS)?;
        let mut batch = WriteBatch::default();
        for addr in addrs {
            batch.delete_cf(cf, address_key(*addr));
        }
        self.db.write(batch)?;
        Ok(())
    }

    pub fn load_all(&self) -> Result<Vec<AddressRecord>> {
        let cf = cf(&self.db, cf::PEERS)?;
        self.db
            .iterator_cf(cf, IteratorMode::Start)
            .map(|item| {
                let (key, value) = item?;
                AddressRecord::from_bytes(&key, &value)
            })
            .collect()
    }
}

/// One banned network range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BanRecord {
    /// Encoded subnet, 16-byte address plus prefix length.
    pub subnet: [u8; 17],
    pub created: i64,
    /// Unix time the ban expires.
    pub until: i64,
    pub reason: String,
}

impl BanRecord {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(20 + self.reason.len());
        bytes.extend_from_slice(&self.created.to_le_bytes());
        bytes.extend_from_slice(&self.until.to_le_bytes());
        let len = u32::try_from(self.reason.len()).unwrap_or(u32::MAX);
        bytes.extend_from_slice(&len.to_le_bytes());
        bytes.extend_from_slice(&self.reason.as_bytes()[..len as usize]);
        bytes
    }

    fn from_bytes(key: &[u8], bytes: &[u8]) -> Result<Self> {
        let corrupted = |reason: &'static str| StorageError::corrupted("ban record", reason);
        let subnet: [u8; 17] = key.try_into().map_err(|_| corrupted("invalid key"))?;
        if bytes.len() < 20 {
            return Err(corrupted("shorter than its header"));
        }
        let created = i64::from_le_bytes(bytes[0..8].try_into().expect("slice is 8 bytes"));
        let until = i64::from_le_bytes(bytes[8..16].try_into().expect("slice is 8 bytes"));
        let len = u32::from_le_bytes(bytes[16..20].try_into().expect("slice is 4 bytes")) as usize;
        let reason = bytes
            .get(20..20 + len)
            .ok_or_else(|| corrupted("reason is truncated"))?;
        let reason = String::from_utf8(reason.to_vec())
            .map_err(|_| corrupted("reason is not valid UTF-8"))?;
        Ok(Self {
            subnet,
            created,
            until,
            reason,
        })
    }
}

/// The ban list. Automatic discouragement is not stored; it stays in memory, as in Core.
#[derive(Clone)]
pub struct BanStore {
    db: Arc<DB>,
}

impl BanStore {
    pub(crate) fn new(db: Arc<DB>) -> Self {
        Self { db }
    }

    pub fn put(&self, record: &BanRecord) -> Result<()> {
        let cf = cf(&self.db, cf::BANS)?;
        self.db.put_cf(cf, record.subnet, record.to_bytes())?;
        Ok(())
    }

    pub fn delete(&self, subnet: &[u8; 17]) -> Result<()> {
        let cf = cf(&self.db, cf::BANS)?;
        self.db.delete_cf(cf, subnet)?;
        Ok(())
    }

    pub fn load_all(&self) -> Result<Vec<BanRecord>> {
        let cf = cf(&self.db, cf::BANS)?;
        self.db
            .iterator_cf(cf, IteratorMode::Start)
            .map(|item| {
                let (key, value) = item?;
                BanRecord::from_bytes(&key, &value)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::test_utils::open_temp;

    fn addr(last: u8, port: u16) -> SocketAddr {
        SocketAddr::from(([10, 0, 0, last], port))
    }

    #[test]
    fn address_roundtrip_through_the_database() {
        let (_dir, storage) = open_temp();
        let store = storage.addresses();

        let mut first = AddressRecord::new(addr(1, 8333), 9, 1000);
        first.tried = true;
        first.attempts = 3;
        first.last_success = 1234;
        let second = AddressRecord::new("[2001:db8::1]:8333".parse().unwrap(), 1, 2000);

        store.put_many(&[first.clone(), second.clone()]).unwrap();
        let mut loaded = store.load_all().unwrap();
        loaded.sort_by_key(|r| r.last_seen);
        assert_eq!(loaded, vec![first.clone(), second.clone()]);

        store.delete_many(&[first.addr]).unwrap();
        assert_eq!(store.load_all().unwrap(), vec![second]);
    }

    #[test]
    fn ban_roundtrip_through_the_database() {
        let (_dir, storage) = open_temp();
        let store = storage.bans();
        let record = BanRecord {
            subnet: [7u8; 17],
            created: 100,
            until: 200,
            reason: "manual".to_string(),
        };

        store.put(&record).unwrap();
        assert_eq!(store.load_all().unwrap(), vec![record.clone()]);

        store.delete(&record.subnet).unwrap();
        assert!(store.load_all().unwrap().is_empty());
    }
}

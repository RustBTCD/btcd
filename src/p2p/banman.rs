//! Bans and discouragement, following Bitcoin Core's split.
//!
//! A ban is manual, covers a subnet, has an expiry and is stored in the database. Config
//! entries act as bans that last while they stay in the config. Discouragement is automatic
//! from misbehaviour, applies to a single address, and is kept in memory only.
//!
//! Core's ban list was once unbounded, which became a memory issue, so both collections here
//! are capped and swept.

use std::collections::HashMap;
use std::net::IpAddr;

use crate::storage::BanRecord;

use super::permissions::Subnet;

/// Upper bound on remembered discouraged addresses.
const MAX_DISCOURAGED: usize = 50_000;

pub struct BanManager {
    /// Bans from the config file: active while configured, never stored.
    configured: Vec<Subnet>,
    /// Stored bans, by encoded subnet.
    bans: HashMap<[u8; 17], (Subnet, BanRecord)>,
    /// Addresses discouraged after misbehaviour, with their expiry.
    discouraged: HashMap<IpAddr, i64>,
    default_duration: i64,
    discourage_duration: i64,
    dirty: Vec<BanRecord>,
    removed: Vec<[u8; 17]>,
}

impl BanManager {
    pub fn new(
        stored: Vec<BanRecord>,
        configured: Vec<Subnet>,
        default_duration: i64,
        discourage_duration: i64,
    ) -> Self {
        let bans = stored
            .into_iter()
            .filter_map(|record| {
                let subnet = Subnet::from_bytes(&record.subnet)?;
                Some((record.subnet, (subnet, record)))
            })
            .collect();
        Self {
            configured,
            bans,
            discouraged: HashMap::new(),
            default_duration,
            discourage_duration,
            dirty: Vec::new(),
            removed: Vec::new(),
        }
    }

    /// Bans a subnet until `now + duration`, or the configured default duration.
    pub fn ban(
        &mut self,
        subnet: Subnet,
        reason: impl Into<String>,
        now: i64,
        duration: Option<i64>,
    ) {
        let record = BanRecord {
            subnet: subnet.to_bytes(),
            created: now,
            until: now.saturating_add(duration.unwrap_or(self.default_duration)),
            reason: reason.into(),
        };
        self.dirty.push(record.clone());
        self.bans.insert(record.subnet, (subnet, record));
    }

    pub fn unban(&mut self, subnet: Subnet) -> bool {
        let key = subnet.to_bytes();
        if self.bans.remove(&key).is_some() {
            self.removed.push(key);
            return true;
        }
        false
    }

    pub fn clear(&mut self) {
        for key in self.bans.keys() {
            self.removed.push(*key);
        }
        self.bans.clear();
    }

    pub fn is_banned(&self, ip: IpAddr, now: i64) -> bool {
        self.configured.iter().any(|subnet| subnet.contains(ip))
            || self
                .bans
                .values()
                .any(|(subnet, record)| record.until > now && subnet.contains(ip))
    }

    /// Marks an address as misbehaving. Bitcoin Core discourages in one strike, with no score.
    pub fn discourage(&mut self, ip: IpAddr, now: i64) {
        if self.discouraged.len() >= MAX_DISCOURAGED {
            self.sweep(now);
        }
        if self.discouraged.len() < MAX_DISCOURAGED {
            self.discouraged
                .insert(ip, now.saturating_add(self.discourage_duration));
        }
    }

    pub fn is_discouraged(&self, ip: IpAddr, now: i64) -> bool {
        self.discouraged.get(&ip).is_some_and(|until| *until > now)
    }

    /// True if we should avoid connecting to this address at all.
    pub fn is_blocked(&self, ip: IpAddr, now: i64) -> bool {
        self.is_banned(ip, now) || self.is_discouraged(ip, now)
    }

    /// Drops expired entries. Expired stored bans are also deleted from the database.
    pub fn sweep(&mut self, now: i64) {
        self.discouraged.retain(|_, until| *until > now);
        let expired: Vec<[u8; 17]> = self
            .bans
            .iter()
            .filter(|(_, (_, record))| record.until <= now)
            .map(|(key, _)| *key)
            .collect();
        for key in expired {
            self.bans.remove(&key);
            self.removed.push(key);
        }
    }

    pub fn list(&self) -> Vec<&BanRecord> {
        self.bans.values().map(|(_, record)| record).collect()
    }

    pub fn take_dirty(&mut self) -> Vec<BanRecord> {
        std::mem::take(&mut self.dirty)
    }

    pub fn take_removed(&mut self) -> Vec<[u8; 17]> {
        std::mem::take(&mut self.removed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    fn manager() -> BanManager {
        BanManager::new(
            vec![],
            vec![Subnet::parse("192.168.0.0/16").unwrap()],
            100,
            50,
        )
    }

    #[test]
    fn configured_bans_apply_without_storage() {
        let bans = manager();
        assert!(bans.is_banned(ip("192.168.1.1"), 0));
        assert!(!bans.is_banned(ip("10.0.0.1"), 0));
        assert!(bans.list().is_empty(), "configured bans are not stored");
    }

    #[test]
    fn bans_expire_and_are_deleted() {
        let mut bans = manager();
        bans.ban(Subnet::parse("10.0.0.0/8").unwrap(), "manual", 1000, None);
        assert!(bans.is_banned(ip("10.1.1.1"), 1000));
        assert_eq!(bans.take_dirty().len(), 1);

        assert!(!bans.is_banned(ip("10.1.1.1"), 1101), "expired");
        bans.sweep(1101);
        assert_eq!(bans.take_removed().len(), 1);
        assert!(bans.list().is_empty());
    }

    #[test]
    fn unban_removes_a_ban() {
        let mut bans = manager();
        let subnet = Subnet::parse("10.0.0.0/8").unwrap();
        bans.ban(subnet, "manual", 1000, Some(10_000));
        assert!(bans.unban(subnet));
        assert!(!bans.unban(subnet), "already gone");
        assert!(!bans.is_banned(ip("10.1.1.1"), 1000));
    }

    #[test]
    fn discouragement_is_temporary_and_not_stored() {
        let mut bans = manager();
        bans.discourage(ip("8.8.8.8"), 1000);
        assert!(bans.is_discouraged(ip("8.8.8.8"), 1000));
        assert!(bans.is_blocked(ip("8.8.8.8"), 1000));
        assert!(
            bans.take_dirty().is_empty(),
            "discouragement is memory only"
        );

        assert!(!bans.is_discouraged(ip("8.8.8.8"), 1051), "expired");
        bans.sweep(1051);
        assert!(!bans.is_blocked(ip("8.8.8.8"), 1051));
    }
}

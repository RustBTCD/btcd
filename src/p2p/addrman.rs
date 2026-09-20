//! The address book, modelled on Bitcoin Core's `AddrMan`.
//!
//! Addresses come from the database, the config file, DNS seeds and `addr` messages. They
//! start in a "new" table and move to "tried" once a connection succeeds. Selection prefers
//! tried addresses, backs off from repeated failures, and keeps one address per network group
//! so a single network cannot fill our connection slots.
//!
//! Simplifications compared with Core: it spreads addresses over fixed bucket sets keyed by a
//! secret, with precise collision and eviction rules, and can group by autonomous system. We
//! keep a flat table, uniform random choice among eligible addresses, and group by address
//! prefix.

use std::collections::{HashMap, HashSet};
use std::net::{IpAddr, SocketAddr};

use bitcoin::secp256k1::rand::seq::IteratorRandom;
use bitcoin::secp256k1::rand::{Rng, thread_rng};

use crate::storage::AddressRecord;

/// Shortest wait before retrying an address that failed, in seconds.
const RETRY_BASE_SECS: i64 = 60;
/// Longest wait between retries, in seconds.
const RETRY_MAX_SECS: i64 = 3600;
/// Chance of picking from the tried table when both tables have candidates.
const TRIED_CHANCE: f64 = 0.5;

/// A network group: Bitcoin Core's `GetGroup`, simplified to address prefixes.
/// IPv4 is grouped by its first 16 bits, IPv6 by its first 32.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Group([u8; 5]);

pub fn group_of(ip: IpAddr) -> Group {
    let mut group = [0u8; 5];
    match ip {
        IpAddr::V4(ipv4) => {
            group[0] = 4;
            group[1..3].copy_from_slice(&ipv4.octets()[..2]);
        }
        IpAddr::V6(ipv6) => match ipv6.to_ipv4_mapped() {
            Some(ipv4) => {
                group[0] = 4;
                group[1..3].copy_from_slice(&ipv4.octets()[..2]);
            }
            None => {
                group[0] = 6;
                group[1..5].copy_from_slice(&ipv6.octets()[..4]);
            }
        },
    }
    Group(group)
}

pub struct AddressManager {
    records: HashMap<SocketAddr, AddressRecord>,
    max_records: usize,
    /// Records changed since the last save.
    dirty: HashSet<SocketAddr>,
    /// Records dropped since the last save.
    removed: Vec<SocketAddr>,
}

impl AddressManager {
    pub fn new(stored: Vec<AddressRecord>, max_records: usize) -> Self {
        let records = stored.into_iter().map(|r| (r.addr, r)).collect();
        Self {
            records,
            max_records: max_records.max(1),
            dirty: HashSet::new(),
            removed: Vec::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    pub fn tried_count(&self) -> usize {
        self.records.values().filter(|r| r.tried).count()
    }

    /// Adds an address we learned about. Returns true if it was not known before.
    pub fn add(&mut self, addr: SocketAddr, services: u64, now: i64) -> bool {
        if !is_routable(addr) {
            return false;
        }
        match self.records.get_mut(&addr) {
            Some(record) => {
                record.last_seen = now;
                if services != 0 {
                    record.services = services;
                }
                self.dirty.insert(addr);
                false
            }
            None => {
                if self.records.len() >= self.max_records && !self.evict_one() {
                    return false;
                }
                self.records
                    .insert(addr, AddressRecord::new(addr, services, now));
                self.dirty.insert(addr);
                true
            }
        }
    }

    pub fn mark_attempt(&mut self, addr: SocketAddr, now: i64) {
        if let Some(record) = self.records.get_mut(&addr) {
            record.last_try = now;
            self.dirty.insert(addr);
        }
    }

    /// A connection succeeded: move the address to the tried table.
    pub fn mark_good(&mut self, addr: SocketAddr, services: u64, now: i64) {
        if let Some(record) = self.records.get_mut(&addr) {
            record.last_success = now;
            record.last_seen = now;
            record.attempts = 0;
            record.tried = true;
            if services != 0 {
                record.services = services;
            }
            self.dirty.insert(addr);
        }
    }

    pub fn mark_failed(&mut self, addr: SocketAddr, now: i64) {
        if let Some(record) = self.records.get_mut(&addr) {
            record.attempts = record.attempts.saturating_add(1);
            record.last_try = now;
            self.dirty.insert(addr);
        }
    }

    /// Picks an address to connect to.
    ///
    /// `feeler` selects only untried addresses, which is what Core's feeler connections test.
    /// `excluded_groups` holds the network groups we are already connected to.
    pub fn select(
        &self,
        feeler: bool,
        now: i64,
        in_use: &HashSet<SocketAddr>,
        excluded_groups: &HashSet<Group>,
        is_banned: impl Fn(IpAddr) -> bool,
    ) -> Option<SocketAddr> {
        let eligible = |record: &&AddressRecord| {
            !in_use.contains(&record.addr)
                && !excluded_groups.contains(&group_of(record.addr.ip()))
                && !is_banned(record.addr.ip())
                && retry_ready(record, now)
        };

        let (tried, new): (Vec<_>, Vec<_>) = self
            .records
            .values()
            .filter(eligible)
            .partition(|record| record.tried);

        let pool = if feeler || tried.is_empty() {
            new
        } else if new.is_empty() || thread_rng().gen_bool(TRIED_CHANCE) {
            tried
        } else {
            new
        };
        pool.into_iter().choose(&mut thread_rng()).map(|r| r.addr)
    }

    /// Records changed since the last call, for persisting.
    pub fn take_dirty(&mut self) -> Vec<AddressRecord> {
        let dirty = std::mem::take(&mut self.dirty);
        dirty
            .into_iter()
            .filter_map(|addr| self.records.get(&addr).cloned())
            .collect()
    }

    /// Addresses dropped since the last call, for deleting.
    pub fn take_removed(&mut self) -> Vec<SocketAddr> {
        std::mem::take(&mut self.removed)
    }

    /// Drops the least useful untried address. Tried addresses are kept.
    fn evict_one(&mut self) -> bool {
        let worst = self
            .records
            .values()
            .filter(|r| !r.tried)
            .min_by_key(|r| (r.last_seen, std::cmp::Reverse(r.attempts)))
            .map(|r| r.addr);
        match worst {
            Some(addr) => {
                self.records.remove(&addr);
                self.dirty.remove(&addr);
                self.removed.push(addr);
                true
            }
            None => false,
        }
    }
}

/// Waits longer after each failed attempt, so dead addresses are retried rarely.
fn retry_ready(record: &AddressRecord, now: i64) -> bool {
    if record.attempts == 0 {
        return true;
    }
    let shift = record.attempts.min(6);
    let wait = (RETRY_BASE_SECS << shift).min(RETRY_MAX_SECS);
    now.saturating_sub(record.last_try) >= wait
}

/// Skips addresses we can never reach: unspecified, loopback beyond explicit config, and
/// multicast. Private ranges stay allowed so regtest and local networks work.
fn is_routable(addr: SocketAddr) -> bool {
    if addr.port() == 0 {
        return false;
    }
    match addr.ip() {
        IpAddr::V4(ip) => !ip.is_unspecified() && !ip.is_multicast() && !ip.is_broadcast(),
        IpAddr::V6(ip) => !ip.is_unspecified() && !ip.is_multicast(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn addr(a: u8, b: u8) -> SocketAddr {
        SocketAddr::from(([10, a, b, 1], 8333))
    }

    fn never_banned(_: IpAddr) -> bool {
        false
    }

    fn select(manager: &AddressManager, feeler: bool, now: i64) -> Option<SocketAddr> {
        manager.select(feeler, now, &HashSet::new(), &HashSet::new(), never_banned)
    }

    #[test]
    fn adds_and_promotes_addresses() {
        let mut manager = AddressManager::new(vec![], 100);
        assert!(manager.add(addr(0, 1), 9, 1000));
        assert!(!manager.add(addr(0, 1), 9, 1100), "second add is not new");
        assert_eq!(manager.len(), 1);
        assert_eq!(manager.tried_count(), 0);

        manager.mark_good(addr(0, 1), 9, 1200);
        assert_eq!(manager.tried_count(), 1);
        assert!(
            select(&manager, true, 1300).is_none(),
            "feelers skip tried addresses"
        );
        assert_eq!(select(&manager, false, 1300), Some(addr(0, 1)));
    }

    #[test]
    fn backs_off_after_failures() {
        let mut manager = AddressManager::new(vec![], 100);
        manager.add(addr(0, 1), 0, 1000);
        manager.mark_failed(addr(0, 1), 1000);

        assert_eq!(select(&manager, false, 1030), None, "still backing off");
        assert_eq!(select(&manager, false, 1200), Some(addr(0, 1)));
    }

    #[test]
    fn skips_addresses_in_use_banned_or_in_a_used_group() {
        let mut manager = AddressManager::new(vec![], 100);
        manager.add(addr(0, 1), 0, 1000);

        let in_use: HashSet<SocketAddr> = [addr(0, 1)].into_iter().collect();
        let empty_groups = HashSet::new();
        assert_eq!(
            manager.select(false, 1000, &in_use, &empty_groups, never_banned),
            None
        );

        let groups: HashSet<Group> = [group_of(addr(0, 1).ip())].into_iter().collect();
        assert_eq!(
            manager.select(false, 1000, &HashSet::new(), &groups, never_banned),
            None
        );
        assert_eq!(
            manager.select(false, 1000, &HashSet::new(), &empty_groups, |_| true),
            None
        );
    }

    #[test]
    fn groups_ignore_the_low_bits() {
        assert_eq!(group_of(addr(0, 1).ip()), group_of(addr(0, 250).ip()));
        assert_ne!(group_of(addr(0, 1).ip()), group_of(addr(9, 1).ip()));
        let v6: SocketAddr = "[2001:db8::1]:8333".parse().unwrap();
        let same: SocketAddr = "[2001:db8:ffff::9]:8333".parse().unwrap();
        assert_eq!(group_of(v6.ip()), group_of(same.ip()));
    }

    #[test]
    fn evicts_untried_addresses_when_full_and_keeps_tried_ones() {
        let mut manager = AddressManager::new(vec![], 2);
        manager.add(addr(0, 1), 0, 1000);
        manager.add(addr(1, 1), 0, 2000);
        manager.mark_good(addr(1, 1), 0, 2000);

        assert!(
            manager.add(addr(2, 1), 0, 3000),
            "evicts the oldest untried address"
        );
        assert_eq!(manager.len(), 2);
        assert_eq!(manager.take_removed(), vec![addr(0, 1)]);
        assert_eq!(manager.tried_count(), 1);
    }

    #[test]
    fn tracks_changes_for_persistence() {
        let mut manager = AddressManager::new(vec![], 100);
        manager.add(addr(0, 1), 0, 1000);
        assert_eq!(manager.take_dirty().len(), 1);
        assert!(manager.take_dirty().is_empty(), "taken changes are cleared");

        manager.mark_attempt(addr(0, 1), 1100);
        assert_eq!(manager.take_dirty().len(), 1);
    }

    #[test]
    fn rejects_unusable_addresses() {
        let mut manager = AddressManager::new(vec![], 100);
        assert!(!manager.add(SocketAddr::from(([10, 0, 0, 1], 0)), 0, 1000));
        assert!(!manager.add(SocketAddr::from(([0, 0, 0, 0], 8333)), 0, 1000));
        assert!(manager.is_empty());
    }
}

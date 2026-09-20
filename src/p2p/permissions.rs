//! Peer permissions, modelled on Bitcoin Core's `-whitelist` flags.
//!
//! Config syntax: `"flag,flag@subnet"`, for example `"noban@127.0.0.1"` or
//! `"noban,download@10.0.0.0/8"`.
//!
//! Most flags only matter once we accept inbound connections and serve data. Only [`NO_BAN`]
//! changes behaviour today; the rest are parsed and stored so the syntax stays stable.

use std::net::{AddrParseError, IpAddr, Ipv6Addr, SocketAddr};

/// A network range, like Bitcoin Core's `CSubNet`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Subnet {
    /// Always stored as IPv6; IPv4 addresses are mapped.
    address: Ipv6Addr,
    /// Prefix length in bits, counted over the IPv6 form.
    prefix: u8,
}

#[derive(Debug, thiserror::Error)]
pub enum SubnetError {
    #[error("invalid address in {input}: {source}")]
    Address {
        input: String,
        source: AddrParseError,
    },

    #[error("invalid prefix length in {0}")]
    Prefix(String),
}

impl Subnet {
    pub fn single(ip: IpAddr) -> Self {
        Self {
            address: to_ipv6(ip),
            prefix: 128,
        }
    }

    pub fn parse(input: &str) -> Result<Self, SubnetError> {
        let (address, prefix) = match input.split_once('/') {
            None => (input, None),
            Some((address, prefix)) => {
                let prefix = prefix
                    .parse::<u8>()
                    .map_err(|_| SubnetError::Prefix(input.to_string()))?;
                (address, Some(prefix))
            }
        };
        let ip: IpAddr = address.parse().map_err(|source| SubnetError::Address {
            input: input.to_string(),
            source,
        })?;

        // A /24 written for an IPv4 address means the last 24 bits of its IPv6 form.
        let max_prefix = if ip.is_ipv4() { 32 } else { 128 };
        let prefix = prefix.unwrap_or(max_prefix);
        if prefix > max_prefix {
            return Err(SubnetError::Prefix(input.to_string()));
        }
        let prefix = if ip.is_ipv4() { prefix + 96 } else { prefix };
        Ok(Self {
            address: to_ipv6(ip),
            prefix,
        })
    }

    pub fn contains(&self, ip: IpAddr) -> bool {
        let address = to_ipv6(ip).octets();
        let network = self.address.octets();
        let full_bytes = (self.prefix / 8) as usize;
        let spare_bits = self.prefix % 8;
        if address[..full_bytes] != network[..full_bytes] {
            return false;
        }
        if spare_bits == 0 {
            return true;
        }
        let mask = 0xffu8 << (8 - spare_bits);
        address[full_bytes] & mask == network[full_bytes] & mask
    }

    /// 17 bytes: the address, then the prefix length.
    pub fn to_bytes(self) -> [u8; 17] {
        let mut bytes = [0u8; 17];
        bytes[..16].copy_from_slice(&self.address.octets());
        bytes[16] = self.prefix;
        bytes
    }

    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        let address: [u8; 16] = bytes.get(..16)?.try_into().ok()?;
        Some(Self {
            address: Ipv6Addr::from(address),
            prefix: *bytes.get(16)?,
        })
    }
}

impl std::fmt::Display for Subnet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.address.to_ipv4_mapped() {
            Some(ipv4) => write!(f, "{ipv4}/{}", self.prefix - 96),
            None => write!(f, "{}/{}", self.address, self.prefix),
        }
    }
}

fn to_ipv6(ip: IpAddr) -> Ipv6Addr {
    match ip {
        IpAddr::V4(ipv4) => ipv4.to_ipv6_mapped(),
        IpAddr::V6(ipv6) => ipv6,
    }
}

/// Permission flags, as in Bitcoin Core's `NetPermissionFlags`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Permissions(u32);

impl Permissions {
    pub const NONE: Self = Self(0);
    /// Never disconnected or banned for misbehaviour. Active today.
    pub const NO_BAN: Self = Self(1 << 0);
    /// May request blocks even when we are busy. Inactive: we serve nothing yet.
    pub const DOWNLOAD: Self = Self(1 << 1);
    /// Transactions are relayed to this peer. Inactive: no transaction relay yet.
    pub const RELAY: Self = Self(1 << 2);
    /// Transactions from this peer bypass relay policy. Inactive.
    pub const FORCE_RELAY: Self = Self(1 << 3);
    /// May query our mempool. Inactive.
    pub const MEMPOOL: Self = Self(1 << 4);
    /// Address messages from this peer bypass rate limits. Inactive.
    pub const ADDR: Self = Self(1 << 5);
    /// May set bloom filters. Inactive.
    pub const BLOOM_FILTER: Self = Self(1 << 6);

    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    pub fn insert(&mut self, other: Self) {
        self.0 |= other.0;
    }

    fn parse_flag(name: &str) -> Option<Self> {
        match name.trim() {
            "noban" => Some(Self::NO_BAN),
            "download" => Some(Self::DOWNLOAD),
            "relay" => Some(Self::RELAY),
            "forcerelay" => Some(Self::FORCE_RELAY),
            "mempool" => Some(Self::MEMPOOL),
            "addr" => Some(Self::ADDR),
            "bloomfilter" => Some(Self::BLOOM_FILTER),
            _ => None,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PermissionError {
    #[error("missing '@subnet' in permission entry {0}")]
    MissingSubnet(String),

    #[error("unknown permission flag {flag} in {input}")]
    UnknownFlag { input: String, flag: String },

    #[error("{0}")]
    Subnet(#[from] SubnetError),
}

/// Permissions granted to peers by address, from the config file.
#[derive(Debug, Clone, Default)]
pub struct PermissionTable {
    entries: Vec<(Subnet, Permissions)>,
}

impl PermissionTable {
    pub fn parse(entries: &[String]) -> Result<Self, PermissionError> {
        let mut table = Vec::new();
        for entry in entries {
            let (flags, subnet) = entry
                .split_once('@')
                .ok_or_else(|| PermissionError::MissingSubnet(entry.clone()))?;
            let mut permissions = Permissions::NONE;
            for flag in flags.split(',') {
                let parsed =
                    Permissions::parse_flag(flag).ok_or_else(|| PermissionError::UnknownFlag {
                        input: entry.clone(),
                        flag: flag.trim().to_string(),
                    })?;
                permissions.insert(parsed);
            }
            table.push((Subnet::parse(subnet)?, permissions));
        }
        Ok(Self { entries: table })
    }

    pub fn for_peer(&self, addr: SocketAddr) -> Permissions {
        let mut permissions = Permissions::NONE;
        for (subnet, granted) in &self.entries {
            if subnet.contains(addr.ip()) {
                permissions.insert(*granted);
            }
        }
        permissions
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    #[test]
    fn subnet_matching() {
        let net = Subnet::parse("10.0.0.0/8").unwrap();
        assert!(net.contains(ip("10.1.2.3")));
        assert!(!net.contains(ip("11.0.0.1")));

        let host = Subnet::parse("127.0.0.1").unwrap();
        assert!(host.contains(ip("127.0.0.1")));
        assert!(!host.contains(ip("127.0.0.2")));

        let odd = Subnet::parse("192.168.1.0/28").unwrap();
        assert!(odd.contains(ip("192.168.1.15")));
        assert!(!odd.contains(ip("192.168.1.16")));

        let v6 = Subnet::parse("2001:db8::/32").unwrap();
        assert!(v6.contains(ip("2001:db8:1::1")));
        assert!(!v6.contains(ip("2001:db9::1")));
    }

    #[test]
    fn subnet_roundtrip_and_display() {
        for input in ["10.0.0.0/8", "127.0.0.1", "2001:db8::/32"] {
            let net = Subnet::parse(input).unwrap();
            assert_eq!(Subnet::from_bytes(&net.to_bytes()), Some(net));
        }
        assert_eq!(
            Subnet::parse("10.0.0.0/8").unwrap().to_string(),
            "10.0.0.0/8"
        );
    }

    #[test]
    fn subnet_rejects_bad_input() {
        assert!(Subnet::parse("not-an-address").is_err());
        assert!(Subnet::parse("10.0.0.0/33").is_err());
        assert!(Subnet::parse("10.0.0.0/x").is_err());
    }

    #[test]
    fn permissions_from_config() {
        let table = PermissionTable::parse(&[
            "noban@127.0.0.1".to_string(),
            "download,addr@10.0.0.0/8".to_string(),
        ])
        .unwrap();

        let local = table.for_peer(SocketAddr::from(([127, 0, 0, 1], 8333)));
        assert!(local.contains(Permissions::NO_BAN));
        assert!(!local.contains(Permissions::DOWNLOAD));

        let lan = table.for_peer(SocketAddr::from(([10, 1, 1, 1], 8333)));
        assert!(lan.contains(Permissions::DOWNLOAD));
        assert!(lan.contains(Permissions::ADDR));
        assert!(!lan.contains(Permissions::NO_BAN));

        let other = table.for_peer(SocketAddr::from(([8, 8, 8, 8], 8333)));
        assert_eq!(other, Permissions::NONE);
    }

    #[test]
    fn permissions_reject_bad_entries() {
        assert!(PermissionTable::parse(&["noban".to_string()]).is_err());
        assert!(PermissionTable::parse(&["nosuchflag@127.0.0.1".to_string()]).is_err());
        assert!(PermissionTable::parse(&["noban@nonsense".to_string()]).is_err());
    }
}

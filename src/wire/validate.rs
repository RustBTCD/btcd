//! Limits a peer must respect, and what we do when it does not.
//!
//! The message crate decodes without bounding how much a peer may send in one message, so
//! these checks run on every decoded message before it is acted on.

use p2p::message_blockdata::Inventory;

use super::limits::{MAX_ADDR_RECORDS, MAX_HEADERS, MAX_INV_VECTORS, MAX_LOCATOR_HASHES};
use super::{Message, Reaction};

/// What a peer sent too many of.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Overflow {
    AddressRecords,
    InventoryVectors,
    Headers,
    LocatorHashes,
}

impl Overflow {
    fn limit(self) -> usize {
        match self {
            Overflow::AddressRecords => MAX_ADDR_RECORDS,
            Overflow::InventoryVectors => MAX_INV_VECTORS,
            Overflow::Headers => MAX_HEADERS,
            Overflow::LocatorHashes => MAX_LOCATOR_HASHES,
        }
    }
}

impl std::fmt::Display for Overflow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            Overflow::AddressRecords => "address records",
            Overflow::InventoryVectors => "inventory vectors",
            Overflow::Headers => "headers",
            Overflow::LocatorHashes => "locator hashes",
        };
        f.write_str(name)
    }
}

/// A message that breaks a limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum WireError {
    #[error("{count} {kind} exceed the limit of {limit}")]
    TooMany {
        kind: Overflow,
        count: usize,
        limit: usize,
    },
}

impl WireError {
    /// What a peer does about this violation.
    ///
    /// Oversized vectors cost us memory the peer had no reason to make us spend, so its
    /// address is remembered. An oversized locator only costs us the message.
    pub fn reaction(&self) -> Reaction {
        match self {
            WireError::TooMany {
                kind: Overflow::LocatorHashes,
                ..
            } => Reaction::Terminate,
            WireError::TooMany { .. } => Reaction::TerminateAndRecord,
        }
    }
}

/// Checks a decoded message against the limits that bound our memory.
pub fn check(message: &Message) -> Result<(), WireError> {
    let (kind, count) = match message {
        Message::Addr(payload) => (Overflow::AddressRecords, payload.0.len()),
        Message::AddrV2(payload) => (Overflow::AddressRecords, payload.0.len()),
        Message::Inv(payload) | Message::GetData(payload) | Message::NotFound(payload) => {
            (Overflow::InventoryVectors, payload.0.len())
        }
        Message::Headers(payload) => (Overflow::Headers, payload.0.len()),
        Message::GetHeaders(request) => (
            Overflow::LocatorHashes,
            request.locator_hashes.hashes().len(),
        ),
        Message::GetBlocks(request) => (
            Overflow::LocatorHashes,
            request.locator_hashes.hashes().len(),
        ),
        _ => return Ok(()),
    };

    let limit = kind.limit();
    if count <= limit {
        Ok(())
    } else {
        Err(WireError::TooMany { kind, count, limit })
    }
}

/// True when a peer tries to use a service we do not advertise.
///
/// Connection bloom filtering and compact block filters are both gated on a service bit. We
/// offer neither, so a peer sending these is asking us for work we never sold it.
pub fn requests_unoffered_service(message: &Message) -> bool {
    matches!(
        message,
        Message::FilterLoad(_)
            | Message::FilterAdd(_)
            | Message::FilterClear
            | Message::MemPool
            | Message::GetCFilters(_)
            | Message::GetCFHeaders(_)
            | Message::GetCFCheckpt(_)
    )
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use bitcoin::hashes::Hash;
    use bitcoin::{BlockHash, Txid};
    use p2p::address::{AddrV1Message, Address};
    use p2p::bip434::{Feature, FeatureData, FeatureId};
    use p2p::message::{AddrPayload, HeadersMessage, InventoryPayload, NetworkHeader};
    use p2p::message_blockdata::{BlockLocator, GetHeadersMessage};
    use p2p::message_network::{
        ClientSoftwareVersion, UserAgent, UserAgentVersion, VersionMessage,
    };
    use p2p::{ProtocolVersion, ServiceFlags};

    pub fn version() -> VersionMessage {
        let address = Address::new(&"1.2.3.4:8333".parse().unwrap(), ServiceFlags::NONE);
        VersionMessage::new(
            ProtocolVersion::FEATURE_VERSION,
            ServiceFlags::WITNESS,
            0,
            address.clone(),
            address,
            7,
            UserAgent::new(
                "rust-btcd",
                &UserAgentVersion::new(ClientSoftwareVersion::SemVer {
                    major: 0,
                    minor: 1,
                    revision: 0,
                }),
            ),
            0,
        )
    }

    pub fn feature() -> Feature {
        Feature {
            feature_id: "test-feature".parse().unwrap(),
            feature_data: FeatureData::new(vec![1, 2, 3]).unwrap(),
        }
    }

    fn address_record() -> AddrV1Message {
        AddrV1Message {
            time: 0,
            address: Address::new(&"1.2.3.4:8333".parse().unwrap(), ServiceFlags::NONE),
        }
    }

    fn inventory() -> Inventory {
        Inventory::Block(BlockHash::from_byte_array([1; 32]))
    }

    fn header_entry() -> NetworkHeader {
        let block = bitcoin::constants::genesis_block(bitcoin::Network::Bitcoin);
        NetworkHeader {
            header: *block.header(),
            length: 0,
        }
    }

    fn get_headers(hashes: usize) -> GetHeadersMessage {
        GetHeadersMessage {
            version: ProtocolVersion::WTXID_RELAY_VERSION,
            locator_hashes: BlockLocator::from(vec![BlockHash::from_byte_array([2; 32]); hashes]),
            stop_hash: BlockHash::from_byte_array([0; 32]),
        }
    }

    #[test]
    fn messages_within_their_limits_pass() {
        assert!(
            check(&Message::Addr(AddrPayload(vec![
                address_record();
                MAX_ADDR_RECORDS
            ])))
            .is_ok()
        );
        assert!(check(&Message::Inv(InventoryPayload(vec![inventory(); 10]))).is_ok());
        assert!(
            check(&Message::Headers(HeadersMessage(vec![
                header_entry();
                MAX_HEADERS
            ])))
            .is_ok()
        );
        assert!(check(&Message::GetHeaders(get_headers(MAX_LOCATOR_HASHES))).is_ok());
        assert!(check(&Message::Verack).is_ok());
    }

    #[test]
    fn oversized_address_messages_are_recorded() {
        let message = Message::Addr(AddrPayload(vec![address_record(); MAX_ADDR_RECORDS + 1]));
        let err = check(&message).unwrap_err();
        assert_eq!(
            err,
            WireError::TooMany {
                kind: Overflow::AddressRecords,
                count: MAX_ADDR_RECORDS + 1,
                limit: MAX_ADDR_RECORDS,
            }
        );
        assert_eq!(err.reaction(), Reaction::TerminateAndRecord);
        assert_eq!(
            err.to_string(),
            "1001 address records exceed the limit of 1000"
        );
    }

    #[test]
    fn oversized_inventory_messages_are_recorded() {
        let message = Message::Inv(InventoryPayload(vec![inventory(); MAX_INV_VECTORS + 1]));
        let err = check(&message).unwrap_err();
        assert!(matches!(
            err,
            WireError::TooMany {
                kind: Overflow::InventoryVectors,
                ..
            }
        ));
        assert_eq!(err.reaction(), Reaction::TerminateAndRecord);

        let message = Message::NotFound(InventoryPayload(vec![inventory(); MAX_INV_VECTORS + 1]));
        assert!(check(&message).is_err());
    }

    #[test]
    fn oversized_header_messages_are_recorded() {
        let message = Message::Headers(HeadersMessage(vec![header_entry(); MAX_HEADERS + 1]));
        let err = check(&message).unwrap_err();
        assert!(matches!(
            err,
            WireError::TooMany {
                kind: Overflow::Headers,
                ..
            }
        ));
        assert_eq!(err.reaction(), Reaction::TerminateAndRecord);
    }

    #[test]
    fn oversized_locators_terminate_without_being_recorded() {
        let err = check(&Message::GetHeaders(get_headers(MAX_LOCATOR_HASHES + 1))).unwrap_err();
        assert!(matches!(
            err,
            WireError::TooMany {
                kind: Overflow::LocatorHashes,
                ..
            }
        ));
        assert_eq!(err.reaction(), Reaction::Terminate);
    }

    #[test]
    fn requests_for_services_we_do_not_offer_are_recognised() {
        assert!(requests_unoffered_service(&Message::MemPool));
        assert!(requests_unoffered_service(&Message::FilterClear));

        assert!(!requests_unoffered_service(&Message::GetAddr));
        assert!(!requests_unoffered_service(&Message::Verack));
        assert!(!requests_unoffered_service(&Message::Inv(
            InventoryPayload(vec![inventory()])
        )));
    }
}

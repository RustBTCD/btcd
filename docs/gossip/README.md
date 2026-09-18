# Bitcoin gossip protocol specification

This directory specifies the peer-to-peer protocol by which Bitcoin nodes discover one another and propagate network addresses, block headers, blocks and transactions.

The specification is implementation independent. It describes the protocol as deployed on the Bitcoin mainnet and its test networks, in terms of observable behaviour on the wire. It names no software product and prescribes no internal data structure except where a structure has externally observable security properties, in which case the property is specified and one construction satisfying it is given as a non-normative example.

**Protocol baseline:** version number 70017. **Reconciled with:** the reference implementation as of September 2026 (release 32.0). Where deployed behaviour changed recently, the module says so.

## 1. Scope

In scope:

- Message framing on both the plaintext and the encrypted transport.
- Connection establishment, capability negotiation, liveness and termination.
- Address gossip: announcement, relay, solicitation and acceptance of network addresses.
- Address storage and peer selection, to the extent that they carry security requirements.
- Bootstrapping a node with no prior knowledge of the network.
- Block propagation: header synchronisation, announcement, download and serving, including compact block relay.
- Transaction propagation: announcement, request scheduling, serving and fee filtering.

Out of scope:

- Consensus rules. Whether a block or transaction is valid is not specified here.
- Transaction acceptance policy beyond what is observable as a relay decision.
- Deprecated subsystems. Connection bloom filtering and compact block filters are specified only to the extent that a node must react correctly when a peer attempts to use them.
- Any remote procedure call, wallet, mining or storage interface.

## 2. Document map

| Module | Contents |
| --- | --- |
| [00-conventions.md](00-conventions.md) | Normative language, rule format, peer reactions, terminology, capabilities and profiles, timers |
| [01-encoding.md](01-encoding.md) | Serialisation primitives: integers, compact size, strings, vectors, hashes, addresses, consensus objects, locators |
| [02-transport-v1.md](02-transport-v1.md) | Plaintext message framing |
| [03-transport-v2.md](03-transport-v2.md) | Encrypted transport: selection, detection, session establishment, packets, fallback |
| [04-connection.md](04-connection.md) | Handshake, negotiation, liveness, deadlines, termination, connection roles |
| [05-messages.md](05-messages.md) | Every message: wire layout, validity window, size limit; unknown, deprecated and withdrawn messages |
| [10-address-gossip.md](10-address-gossip.md) | Announcing, relaying, soliciting and accepting addresses |
| [11-peer-management.md](11-peer-management.md) | Peer selection, diversity, reachability verification, inbound acceptance, eviction |
| [12-bootstrap.md](12-bootstrap.md) | Cold start: prior peers, seeds, configured peers |
| [20-block-relay.md](20-block-relay.md) | Header synchronisation, block announcement, download and serving |
| [21-compact-blocks.md](21-compact-blocks.md) | Compact block relay |
| [22-transaction-relay.md](22-transaction-relay.md) | Transaction announcement, request scheduling, serving, fee filtering |
| [30-security.md](30-security.md) | Threat model and the rationale for every non-obvious rule |
| [90-parameters.md](90-parameters.md) | Every constant, limit, timer and network parameter. Normative for values. |
| [91-conformance.md](91-conformance.md) | Rule index per capability, implementation order, interoperability checks |

A reader implementing from scratch should read modules 00 through 05 in order. Modules 10 through 22 are independent of one another. Module 30 explains why the rules are what they are and is not required reading for conformance.

## 3. Requirement identifiers

Every rule carries a stable identifier of the form `PREFIX-nn`, for example `ENC-26`. Identifiers are never reused or renumbered. Gaps in a sequence are intentional and carry no meaning.

| Prefix | Module |
| --- | --- |
| `ENC` | Encoding |
| `T1` | Plaintext transport |
| `T2` | Encrypted transport |
| `CON` | Connection lifecycle |
| `MSG` | Message handling |
| `ADR` | Address gossip |
| `SEL` | Peer management |
| `BST` | Bootstrap |
| `BLK` | Block relay |
| `CMP` | Compact blocks |
| `TXR` | Transaction relay |

## 4. Normative references

- RFC 2119 and RFC 8174, requirement key words.
- BIP 14, Protocol Version and User Agent.
- BIP 31, Pong message.
- BIP 130, sendheaders message.
- BIP 133, feefilter message.
- BIP 144, Segregated Witness peer services.
- BIP 152, Compact Block Relay.
- BIP 155, addrv2 message.
- BIP 159, NODE_NETWORK_LIMITED service bit.
- BIP 324, Version 2 P2P Encrypted Transport Protocol.
- BIP 330, Transaction announcements reconciliation (negotiation only).
- BIP 339, WTXID-based transaction relay.
- BIP 434, Feature negotiation message.

Where this specification and a referenced BIP disagree on an encoding, the BIP is authoritative and the disagreement is a defect in this document.

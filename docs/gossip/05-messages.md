# 05. Messages

Every message a conforming node may encounter: its wire layout, when it is valid, and its size limit. Behaviour rules live in the module named in the catalogue. This module also fixes the handling of unknown, deprecated and withdrawn messages.

## 1. Validity windows

| Window | Meaning |
| --- | --- |
| `HS` | The `version` and `verack` exchange itself. |
| `NEG` | Only between the peer's `version` and its `verack`. After `verack`: terminate. |
| `NEG+EST` | From the peer's `version` onwards. |
| `EST` | Only after the handshake completes. Received earlier: ignored (`CON-48`). |

## 2. Layouts

Types are defined in [01-encoding.md](01-encoding.md).

### 2.1 `version`

| Field | Size | Type | Description |
| --- | --- | --- | --- |
| `version` | 4 | `int32` | Protocol version the sender speaks. |
| `services` | 8 | `uint64` | Service flags the sender claims. |
| `timestamp` | 8 | `int64` | Sender's clock, seconds since the Unix epoch. |
| `addr_recv` | 26 | address | The recipient's address as the sender observes it. |
| `addr_from` | 26 | address | Vestigial. Zeros. |
| `nonce` | 8 | `uint64` | Random value identifying this connection attempt. |
| `user_agent` | var | byte string | BIP 14 form. At most 256 bytes. |
| `start_height` | 4 | `int32` | Height of the sender's best chain. Unauthenticated. |
| `relay` | 1 | `bool` | Whether the sender wants transaction announcements. |

### 2.2 Empty messages

`verack`, `wtxidrelay`, `sendaddrv2`, `sendheaders`, `getaddr`, `mempool`, `filterclear`.

### 2.3 `feature` (BIP 434)

| Field | Size | Type | Description |
| --- | --- | --- | --- |
| `feature_id` | var | byte string | 4 to 80 bytes. Names the feature. |
| `feature_data` | var | byte string | At most 512 bytes. Feature-defined. |

No bytes may follow `feature_data`. No feature identifier is defined by this specification.

### 2.4 `sendtxrcncl` (BIP 330)

`version` (4, `uint32`), `salt` (8, `uint64`). Only a node implementing BIP 330 sends it.

### 2.5 `sendcmpct`

| Field | Size | Type | Description |
| --- | --- | --- | --- |
| `high_bandwidth` | 1 | `uint8` | 0 or 1. Whether the sender wants unsolicited compact blocks. |
| `version` | 8 | `uint64` | Mechanism version. 2 is the only supported value. |

### 2.6 `ping`, `pong`

`nonce` (8, `uint64`).

### 2.7 `addr`, `addrv2`

`addr`: vector of version 1 addresses with timestamp (30 bytes each). `addrv2`: vector of version 2 addresses. At most 1000 records either way.

### 2.8 `inv`, `getdata`, `notfound`

Vector of inventory vectors, at most 50,000.

### 2.9 `getheaders`, `getblocks`

A block locator.

### 2.10 `headers`

Vector of at most 2000 entries, each an 80-byte header followed by a compact size zero (`ENC-25`).

### 2.11 `block`, `tx`

A single block or transaction in the form selected by the request (`ENC-21`).

### 2.12 `feefilter`

`fee_rate` (8, `int64`): satoshis per 1000 virtual bytes.

### 2.13 `cmpctblock`

| Field | Size | Type | Description |
| --- | --- | --- | --- |
| `header` | 80 | block header | |
| `nonce` | 8 | `uint64` | Randomises the short identifiers. |
| `shortids` | var | vector of 6-byte ids | In block order, for transactions not prefilled. |
| `prefilled` | var | vector of entries | Transactions sent in full, witness form. |

A prefilled entry is a compact size index followed by a transaction. Indexes are differential: the first is absolute; each later one is its index minus the previous index minus one.

Short identifiers: compute `SHA256(header ‖ nonce)`; take the first 16 bytes as two little-endian 64-bit SipHash keys; for each transaction, SipHash-2-4 of its wtxid; transmit the low 48 bits little-endian.

### 2.14 `getblocktxn`, `blocktxn`

`getblocktxn`: block hash (32) then a non-empty vector of differentially encoded indexes, as above. `blocktxn`: block hash (32) then a vector of transactions in witness form.

## 3. Catalogue

| Message | Module | Window | Limit | Notes |
| --- | --- | --- | --- | --- |
| `version` | [04](04-connection.md) | `HS` | 256-byte user agent | One per direction. |
| `verack` | [04](04-connection.md) | `HS` | | |
| `wtxidrelay` | [04](04-connection.md), [22](22-transaction-relay.md) | `NEG` | | |
| `sendaddrv2` | [04](04-connection.md), [10](10-address-gossip.md) | `NEG` | | |
| `feature` | [04](04-connection.md) | `NEG` | 80 + 512 bytes | Common version ≥ 70017 only. |
| `sendtxrcncl` | section 7 | `NEG` | | BIP 330. |
| `sendheaders` | [20](20-block-relay.md) | `NEG+EST` | | |
| `sendcmpct` | [21](21-compact-blocks.md) | `NEG+EST` | | |
| `ping`, `pong` | [04](04-connection.md) | `EST` | | |
| `getaddr` | [10](10-address-gossip.md) | `EST` | | Answered once per connection. |
| `addr`, `addrv2` | [10](10-address-gossip.md) | `EST` | 1000 records | `addrv2` accepted whether or not `sendaddrv2` was sent. |
| `inv`, `getdata` | [20](20-block-relay.md), [22](22-transaction-relay.md) | `EST` | 50,000 vectors | |
| `notfound` | [22](22-transaction-relay.md) | `EST` | 50,000 vectors | Transactions only. |
| `getheaders` | [20](20-block-relay.md) | `EST` | 101 locator hashes | |
| `headers` | [20](20-block-relay.md) | `EST` | 2000 headers | |
| `block`, `tx` | [20](20-block-relay.md), [22](22-transaction-relay.md) | `EST` | 4,000,000 bytes | |
| `feefilter` | [22](22-transaction-relay.md) | `EST` | | |
| `cmpctblock`, `getblocktxn`, `blocktxn` | [21](21-compact-blocks.md) | `EST` | 4,000,000 bytes | |
| `getblocks` | section 6 | `EST` | 101 locator hashes | Legacy. |
| `mempool`, `filterload`, `filteradd`, `filterclear`, `merkleblock` | section 5 | `EST` | | Deprecated. |
| `getcfilters`, `cfilter`, `getcfheaders`, `cfheaders`, `getcfcheckpt`, `cfcheckpt` | section 5 | `EST` | | Out of scope. |
| `alert`, `reject` | section 6 | | | Withdrawn. |

## 4. Unknown messages

| ID | Rule | Reaction |
| --- | --- | --- |
| `MSG-10` | MUST ignore a message whose type it does not recognise, in any state, and MUST NOT terminate for it. This is the sole extension mechanism of the protocol. | — |

## 5. Deprecated subsystems

Two serving subsystems exist on the network that this specification does not define. It defines only how a node reacts when a peer attempts to use them.

**Connection bloom filtering** (`filterload`, `filteradd`, `filterclear`, `merkleblock`, `mempool`, `MSG_FILTERED_BLOCK`) is gated on `NODE_BLOOM`. **Compact block filters** (`getcfilters`, `getcfheaders`, `getcfcheckpt` and their responses) are gated on `NODE_COMPACT_FILTERS`.

| ID | Rule | Reaction |
| --- | --- | --- |
| `MSG-12` | MUST NOT send a bloom filtering message, or request `MSG_FILTERED_BLOCK`, to a peer that does not advertise `NODE_BLOOM`. | terminate |
| `MSG-13` | When not advertising `NODE_BLOOM`, MUST terminate a connection on which it receives any bloom filtering message or a `MSG_FILTERED_BLOCK` request. | — |
| `MSG-14` | MUST ignore `merkleblock`. | — |
| `MSG-15` | MUST NOT send a compact filter request to a peer that does not advertise `NODE_COMPACT_FILTERS`. | terminate |
| `MSG-16` | When not serving compact filters, MUST ignore `cfilter`, `cfheaders` and `cfcheckpt`, and MUST terminate a connection on which it receives a compact filter request. | — |

## 6. Withdrawn and legacy messages

`alert` was a signed broadcast channel with a single trusted key. `reject` disclosed validation results. Both are withdrawn. `getblocks` requests block announcements by inventory and predates header-first synchronisation.

| ID | Rule | Reaction |
| --- | --- | --- |
| `MSG-17` | MUST ignore `alert` and `reject`. | — |
| `MSG-18` | MUST NOT send `alert` or `reject`. | ignore |
| `MSG-20` | MUST NOT terminate because a peer sent `getblocks`. MAY answer it with an `inv` of at most 500 block hashes, or ignore it. A node uses `getheaders` instead. | — |

## 7. Transaction reconciliation

`sendtxrcncl` negotiates BIP 330 set reconciliation. The negotiation is deployed; the reconciliation rounds are not on by default in any implementation.

| ID | Rule | Reaction |
| --- | --- | --- |
| `MSG-23` | When not implementing BIP 330, MUST ignore `sendtxrcncl`. When implementing it, MUST follow BIP 330. Peers that implement it terminate when `sendtxrcncl` arrives after `verack`, twice, or on a connection where either side signalled no transaction relay. | — |

## 8. Ordering

| ID | Rule | Reaction |
| --- | --- | --- |
| `MSG-24` | MUST NOT assume responses arrive in the order requests were sent, except where a module states an ordering guarantee. | safety |
| `MSG-25` | MUST tolerate an unsolicited message of any type valid in the current state, including `block`, `tx`, `headers` and `addr` it did not request, and MUST NOT terminate for the absence of a matching request. Exceptions are stated in [21-compact-blocks.md](21-compact-blocks.md) and `CON-46`. | — |

# 01. Encoding

Every message payload is built from the types defined here. Unless a field states otherwise, multi-byte integers are little-endian.

## 1. Integers and booleans

| Type | Size | Encoding |
| --- | --- | --- |
| `uint8`, `int8` | 1 | as is |
| `uint16`, `int16` | 2 | little-endian |
| `uint32`, `int32` | 4 | little-endian |
| `uint64`, `int64` | 8 | little-endian |
| `uint16be` | 2 | **big-endian**; used only for port numbers |
| `bool` | 1 | `0x00` false, any other value true |

Signed integers are two's complement.

| ID | Rule | Reaction |
| --- | --- | --- |
| `ENC-01` | MUST encode ports big-endian and every other integer little-endian. | discard |
| `ENC-02` | MUST encode true as `0x01`. | — (peers accept any non-zero value) |
| `ENC-03` | MUST accept any non-zero byte as true, except where a message restricts the values (`CMP-01`). | discard |

## 2. Compact size

An unsigned integer in a variable-width encoding, used for counts and lengths.

| First byte | Total size | Value range | Encoding |
| --- | --- | --- | --- |
| `0x00`–`0xFC` | 1 | 0 to 252 | the byte itself |
| `0xFD` | 3 | 253 to 65535 | `0xFD` then `uint16` |
| `0xFE` | 5 | 65536 to 2^32−1 | `0xFE` then `uint32` |
| `0xFF` | 9 | 2^32 to 2^64−1 | `0xFF` then `uint64` |

| ID | Rule | Reaction |
| --- | --- | --- |
| `ENC-04` | MUST use the shortest form that represents the value. | discard |
| `ENC-05` | MUST reject a compact size that is not in shortest form or that exceeds 33,554,432 (`0x02000000`). | discard (the whole message) |
| `ENC-06` | MUST NOT allocate memory proportional to a length or count prefix before that many bytes have arrived. | safety |

## 3. Byte strings, text and vectors

A **byte string** is a compact size length followed by that many bytes. **Text** is a byte string interpreted as UTF-8; a stated maximum is a byte count excluding the prefix. A node MUST NOT assume received text is valid UTF-8, printable, or free of null bytes.

A **vector** is a compact size element count followed by that many elements.

| ID | Rule | Reaction |
| --- | --- | --- |
| `ENC-08` | MUST NOT exceed any per-message limit in [90-parameters.md](90-parameters.md) section 3, and on receipt MUST apply the reaction listed there. | as listed |

## 4. Hashes and identifiers

All hashes on the wire are 32 bytes, transmitted in **internal byte order**, the reverse of the display order.

- `SHA256d(x)` = `SHA256(SHA256(x))`: block hashes, transaction identifiers, message checksums.
- `SHA256(x)`: compact block short identifier derivation only.

Two transaction identifiers exist:

- **txid** = `SHA256d` of the transaction in non-witness form (section 9.2).
- **wtxid** = `SHA256d` of the transaction in witness form.

They are equal for a transaction without witness data. They are distinct namespaces on the wire ([22-transaction-relay.md](22-transaction-relay.md)).

## 5. Service flags

A 64-bit field, encoded as `uint64` unless stated otherwise.

| Bit | Value | Name | Meaning |
| --- | --- | --- | --- |
| 0 | 1 | `NODE_NETWORK` | Serves every block of the best chain. |
| 1 | 2 | *(obsolete)* | MUST NOT be set. |
| 2 | 4 | `NODE_BLOOM` | Accepts connection bloom filters. Deprecated ([05-messages.md](05-messages.md) section 4). |
| 3 | 8 | `NODE_WITNESS` | Serves blocks and transactions with witness data. |
| 6 | 64 | `NODE_COMPACT_FILTERS` | Serves compact block filters. Out of scope. |
| 10 | 1024 | `NODE_NETWORK_LIMITED` | Serves at least the most recent 288 blocks. |
| 11 | 2048 | `NODE_P2P_V2` | Accepts the version 2 transport on its listening port. |

Bits 24 to 31 are reserved for experiments.

| ID | Rule | Reaction |
| --- | --- | --- |
| `ENC-09` | MUST ignore service bits it does not recognise. | — |
| `ENC-11` | MUST treat service flags as unauthenticated and MUST tolerate a peer that advertises a capability and then fails to provide it. The only remedy is to stop using that peer for that purpose. | safety |

## 6. Network addresses

### 6.1 Version 1 address, 26 bytes

Used inside `version`.

| Field | Size | Type | Description |
| --- | --- | --- | --- |
| `services` | 8 | `uint64` | Service flags claimed for this address. |
| `address` | 16 | bytes | IPv6, or IPv4 in mapped form `00 ×10, ff, ff, a, b, c, d`. |
| `port` | 2 | `uint16be` | TCP port. |

### 6.2 Version 1 address with timestamp, 30 bytes

Used inside `addr`. As above, preceded by `time` (4, `uint32`): seconds since the Unix epoch at which the address was last believed reachable. The field is unsigned; a node MUST NOT interpret a large value as negative.

### 6.3 Version 2 address, variable length

Used inside `addrv2` (BIP 155).

| Field | Size | Type | Description |
| --- | --- | --- | --- |
| `time` | 4 | `uint32` | As above. |
| `services` | var | compact size | Service flags. Note the encoding differs from the version 1 form. |
| `network_id` | 1 | `uint8` | Table below. |
| `address` | var | byte string | Length fixed by `network_id`. |
| `port` | 2 | `uint16be` | TCP port. |

| `network_id` | Network | Length | Notes |
| --- | --- | --- | --- |
| 1 | IPv4 | 4 | |
| 2 | IPv6 | 16 | |
| 3 | Tor v2 | 10 | Retired. |
| 4 | Tor v3 | 32 | Ed25519 public key, without version byte or checksum. |
| 5 | I2P | 32 | Binary form of the b32 destination. |
| 6 | CJDNS | 16 | First byte `0xFC`. |

| ID | Rule | Reaction |
| --- | --- | --- |
| `ENC-12` | MUST reject an `addrv2` entry whose `address` length does not match its `network_id`, for identifiers 1 through 6. | discard (the whole message) |
| `ENC-13` | MUST skip an entry whose `network_id` it does not recognise, provided `address` is at most 512 bytes, and continue with the next entry. | — |
| `ENC-14` | MUST NOT store, relay or connect to a Tor v2 address. | safety |
| `ENC-15` | MUST reject an `addrv2` entry whose `address` exceeds 512 bytes. | discard (the whole message) |

### 6.4 Usable addresses

| ID | Rule | Reaction |
| --- | --- | --- |
| `ENC-18` | MUST NOT store, relay, or initiate a connection to an address that is not globally routable, whose port is zero, whose network this node cannot reach, or that it has recorded under a *terminate and record* reaction. Not routable includes at least: the unspecified address, loopback, private, link-local, shared address space and documentation ranges. | safety |

## 7. Inventory vectors

A 36-byte structure: `type` (4, `uint32`) then `hash` (32).

| Value | Name | Identifier | Valid in |
| --- | --- | --- | --- |
| 1 | `MSG_TX` | txid | `inv`, `getdata`, `notfound` |
| 2 | `MSG_BLOCK` | block hash | `inv`, `getdata` |
| 3 | `MSG_FILTERED_BLOCK` | block hash | `getdata` only. Deprecated. |
| 4 | `MSG_CMPCT_BLOCK` | block hash | `getdata` only. |
| 5 | `MSG_WTX` | wtxid | `inv`, `getdata`, `notfound` |
| `0x40000001` | `MSG_WITNESS_TX` | txid | `getdata` only. |
| `0x40000002` | `MSG_WITNESS_BLOCK` | block hash | `getdata` only. |

Bit 30 (`0x40000000`) is the witness flag.

| ID | Rule | Reaction |
| --- | --- | --- |
| `ENC-20` | MUST ignore an inventory vector whose `type` it does not recognise and continue with the remaining vectors. | — |

## 8. Block header, 80 bytes

| Field | Size | Type |
| --- | --- | --- |
| `version` | 4 | `int32` |
| `prev_block` | 32 | hash |
| `merkle_root` | 32 | hash |
| `timestamp` | 4 | `uint32` |
| `bits` | 4 | `uint32` |
| `nonce` | 4 | `uint32` |

The block hash is `SHA256d` of these 80 bytes.

## 9. Transactions and blocks

The internal structure of transactions and blocks is fixed by consensus. This section fixes only which of two serialisations is used.

### 9.1 Non-witness form

`version (int32)`, `tx_in` vector, `tx_out` vector, `lock_time (uint32)`.

### 9.2 Witness form (BIP 144)

`version (int32)`, marker `0x00`, flag `0x01`, `tx_in` vector, `tx_out` vector, one witness field per input, `lock_time (uint32)`.

The marker and flag occupy the position of the `tx_in` count. A transaction with zero inputs is invalid, so a parser that does not understand the witness form rejects it rather than misreading it.

### 9.3 Block

An 80-byte header, a compact size transaction count, and that many transactions.

| ID | Rule | Reaction |
| --- | --- | --- |
| `ENC-21` | MUST serialise a transaction or block in the form the request selects: witness form for `MSG_WTX`, `MSG_WITNESS_TX`, `MSG_WITNESS_BLOCK`, and for every transaction inside `cmpctblock` and `blocktxn`; non-witness form for `MSG_TX` and `MSG_BLOCK`. | deprioritise (a witness-stripped object cannot be validated or relayed; the requester stops using the peer) |
| `ENC-23` | MUST accept both forms on receipt and determine which is present from the marker and flag bytes, not from negotiated capabilities. | safety |
| `ENC-25` | Inside `headers`, MUST follow each 80-byte header with a compact size transaction count set to zero, and MUST ignore the value on receipt. | discard |

## 10. Block locator

Used by `getheaders` and `getblocks`.

| Field | Size | Type | Description |
| --- | --- | --- | --- |
| `version` | 4 | `uint32` | Sender's protocol version. Ignored. |
| `locator` | var | vector of hashes | Block hashes, newest first. |
| `hash_stop` | 32 | hash | Stop at this block, or all zeros for no limit. |

| ID | Rule | Reaction |
| --- | --- | --- |
| `ENC-26` | MUST NOT include more than 101 hashes in a locator. | terminate |
| `ENC-27` | SHOULD begin the locator with the tip and step backwards, doubling the step after the first ten entries, ending with the genesis hash. | — |

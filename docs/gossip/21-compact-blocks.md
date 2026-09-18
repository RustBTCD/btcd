# 21. Compact block relay

Capability tag: `[CMP]`. Defined by BIP 152, which is authoritative for the encodings. Layouts: [05-messages.md](05-messages.md) sections 2.5, 2.13 and 2.14.

A compact block transmits a block as a header plus short identifiers for its transactions, relying on the receiver already holding most of them. Only version 2 of the mechanism, which identifies transactions by wtxid, is in scope. Version 1 MUST NOT be used.

## 1. `sendcmpct`

| ID | Rule | Reaction |
| --- | --- | --- |
| `CMP-01` | MUST set `high_bandwidth` to 0 or 1. | terminate and record |
| `CMP-02` | MUST ignore a `sendcmpct` whose `version` it does not support, leaving the peer's state unchanged. A peer that has sent only unsupported versions is treated as not supporting compact blocks. | — |
| `CMP-03` | MUST accept `sendcmpct` at any point after the peer's `version`, repeatedly, applying the most recent value. | — |
| `CMP-04` | With `[CMP]`, MUST send `sendcmpct` with `high_bandwidth` 0 and `version` 2 after the handshake. | — (a peer that has received no `sendcmpct` never requests compact blocks) |

## 2. `cmpctblock`

| ID | Rule | Reaction |
| --- | --- | --- |
| `CMP-05` | MUST include the coinbase transaction as a prefilled entry. | — |
| `CMP-06` | MUST derive short identifiers from the wtxid. | deprioritise (reconstruction fails) |
| `CMP-07` | MUST choose `nonce` afresh and unpredictably for each compact block it constructs. | safety |
| `CMP-08` | MUST validate the header, including proof of work, before acting on a compact block (`BLK-04`). | safety |
| `CMP-09` | MUST terminate and record a peer whose compact block cannot be parsed, or whose prefilled indexes are not strictly increasing or exceed the transaction count. | — |

A short identifier collision within one compact block is not the sender's fault; the receiver requests the full block.

## 3. `getblocktxn` and `blocktxn`

| ID | Rule | Reaction |
| --- | --- | --- |
| `CMP-11` | MUST NOT send `getblocktxn` with an empty index vector. | terminate |
| `CMP-12` | MUST send strictly increasing indexes in `getblocktxn`. | discard |
| `CMP-13` | MUST terminate and record a peer whose `getblocktxn` names an index beyond the last transaction of the block. | — |
| `CMP-14` | MUST answer `getblocktxn` with a `blocktxn` containing exactly the requested transactions, in order, in witness form; or with the full `block` (`CMP-18`). | terminate and record |
| `CMP-15` | MUST NOT send `blocktxn` unsolicited, and MUST NOT send a second `blocktxn` for a block whose reconstruction already failed. | ignore (unsolicited); terminate and record (repeated) |
| `CMP-16` | MUST ignore a `blocktxn` for a block it is not reconstructing. | — |
| `CMP-29` | MUST tolerate `getblocktxn` receiving no response, which is what a peer sends for a block it does not have, and MUST apply its own timeout. | — |

## 4. Requesting compact blocks

| ID | Rule | Reaction |
| --- | --- | --- |
| `CMP-20` | MUST NOT request `MSG_CMPCT_BLOCK` from a peer that has not sent `sendcmpct` with a supported version. | ignore |
| `CMP-18` | MAY answer a `getblocktxn`, or a `getdata` for `MSG_CMPCT_BLOCK`, with the full `block` instead, and MUST tolerate receiving one. Responders do so for blocks more than a few blocks below their tip. | — |

## 5. High-bandwidth mode

A peer that sets `high_bandwidth` asks to receive compact blocks unsolicited, without a prior header announcement. This saves a round trip and is how blocks propagate fastest.

| ID | Rule | Reaction |
| --- | --- | --- |
| `CMP-22` | SHOULD designate at most three peers, preferring ones it connected to itself that have recently delivered new blocks quickly, to which it sends `sendcmpct` with `high_bandwidth` 1. | — |
| `CMP-24` | MUST ignore an unsolicited `cmpctblock` from a peer to which it has not sent `sendcmpct` with `high_bandwidth` 1, unless it is a response to its own request. | — |
| `CMP-25` | MAY send an unsolicited `cmpctblock` to a high-bandwidth peer after validating only the header and its proof of work, before full validation, provided the common version is at least 70015. | — |
| `CMP-26` | MUST NOT terminate or record a peer because a block received as, or reconstructed from, a compact block proved invalid. The header remains attributable and `BLK-04` applies. This is the exception to `BLK-27`. | — |

A `cmpctblock` is used to announce a single block; several blocks at once are announced by `headers`.

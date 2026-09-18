# 20. Block relay

Block propagation proceeds header first. A node learns of a block as a header, validates the header's proof of work against its own chain, and only then requests the block. No node is required to accept an unvalidated block, and none may be compelled to store one.

Message layouts: [05-messages.md](05-messages.md) sections 2.8 to 2.11.

## 1. Header synchronisation

| ID | Rule | Reaction |
| --- | --- | --- |
| `BLK-01` | MUST answer every `getheaders`, with headers or with an empty `headers`. An empty answer is the correct way to decline, for example while its own chain has too little work to be useful. | terminate (an outbound peer that does not answer is removed under `SEL-22`; the initial synchronisation peer is removed at the headers deadline of [90-parameters.md](90-parameters.md) section 5) |
| `BLK-05` | MUST answer with headers beginning after the first locator entry it recognises on its best chain, continuing along that chain, stopping at `hash_stop` or after 2000 headers. If no entry is recognised, MUST begin at the genesis block. If the locator is empty, MUST answer with the single header at `hash_stop`, or with nothing if that block is unknown. | deprioritise |
| `BLK-03` | MUST send headers forming an unbroken sequence, each `prev_block` equal to the hash of the preceding header in the message. | terminate and record |
| `BLK-04` | MUST send only headers carrying valid proof of work for their stated target, in any message including `cmpctblock`. | terminate and record |
| `BLK-06` | MUST treat a `headers` message with fewer than 2000 headers as the sender having no further headers to offer. This is the sole termination condition of header synchronisation. | — |
| `BLK-07` | MUST NOT send fewer than 2000 headers while it has further connected headers on the same chain. | deprioritise (the peer concludes synchronisation is complete and falls behind) |
| `BLK-10` | On receiving a header whose predecessor is unknown, MUST respond with `getheaders`, and MUST NOT treat the unconnected header as a violation. | — |
| `BLK-11` | MUST NOT commit storage to a chain of headers until it has demonstrated cumulative work comparable to the node's own best chain, and MUST NOT terminate the connection for offering one. | safety |
| `BLK-12` | MUST NOT terminate a connection because a peer offered a chain with less work than its own, or one it does not follow, except: an outbound peer during initial synchronisation whose complete chain has less work than the node's minimum-work threshold; and a peer removed under `SEL-22`. | — |

## 2. Announcing blocks

A new block is announced in one of three forms, chosen by what the peer requested.

| Form | Used when |
| --- | --- |
| `headers` | The peer sent `sendheaders`. |
| `cmpctblock` | The peer requested high-bandwidth compact blocks ([21-compact-blocks.md](21-compact-blocks.md)). |
| `inv` with `MSG_BLOCK` | Otherwise, or when the header form would not connect. |

| ID | Rule | Reaction |
| --- | --- | --- |
| `BLK-13` | MUST send `sendheaders` to every peer after the handshake. | — (announcement by `inv` costs an extra round trip) |
| `BLK-14` | SHOULD announce by `headers` to a peer that sent `sendheaders`, at most about eight at a time, and fall back to `inv` when the headers would not connect to a block the peer is believed to have. | — |
| `BLK-16` | MUST announce only blocks it has validated and accepted onto its best chain, except under `CMP-25`. | terminate and record (peers hold the announcer responsible, `BLK-27`) |

## 3. Requesting blocks

| ID | Rule | Reaction |
| --- | --- | --- |
| `BLK-19` | MUST NOT request a block by `getdata` in response to an `inv` before obtaining and validating its header. The response to an `inv` for an unknown block is `getheaders`. | safety |
| `BLK-20` | MUST request blocks as `MSG_WITNESS_BLOCK` from any peer advertising `NODE_WITNESS`. | deprioritise (a witness-stripped block cannot be validated) |
| `BLK-24` | MUST NOT request from a peer advertising `NODE_NETWORK_LIMITED` without `NODE_NETWORK` a block more than 286 blocks below that peer's announced tip. | terminate (rather than answer, which would disclose the peer's retained depth) |
| `BLK-25` | MUST tolerate a `getdata` for a block receiving no response, and MUST apply its own timeout. `notfound` is never sent for blocks. | — |

A requester keeps about 16 blocks outstanding per peer, requests each block from one peer at a time, and downloads from several peers in parallel during initial synchronisation ([90-parameters.md](90-parameters.md) section 3).

## 4. Receiving blocks

| ID | Rule | Reaction |
| --- | --- | --- |
| `BLK-26` | MUST validate every received block before relaying it, storing it as part of its best chain, or acting on it. | safety |
| `BLK-27` | MUST terminate and record a peer that sends a block failing consensus validation, whose transactions do not match its header's commitment, or whose predecessor the node does not have; except as `CMP-26` provides. | — |
| `BLK-28` | MUST accept an unsolicited `block` without terminating, and MAY discard it. | — |
| `BLK-29` | MUST NOT commit validation effort to an unrequested block unless it has at least as much work as the node's tip and is not far ahead of it. | safety |

## 5. Serving blocks

Applies to `[BSA]` and `[BSL]`.

| ID | Rule | Reaction |
| --- | --- | --- |
| `BLK-30` | Advertising `NODE_NETWORK`, MUST serve any block of its best chain on request. Advertising `NODE_NETWORK_LIMITED`, MUST serve at least the most recent 288 blocks. | terminate (for stalling, `CON-01`) |
| `BLK-31` | MUST serve requested blocks promptly and in the order requested. A requester stops waiting after a few seconds when a block is the only obstacle to its progress, and terminates after about ten minutes, extended per parallel peer, when a batch is undelivered. | terminate |
| `BLK-34` | MAY decline to serve blocks older than about a week once an operator-set upload limit is reached, and MAY terminate the connection requesting them. | — |
| `BLK-36` | MUST NOT serve a block it has not validated. A block not on its best chain is served only if validated and less than about a month old. | safety |

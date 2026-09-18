# 22. Transaction relay

Capability tag: `[TXR]`.

Unconfirmed transactions are announced by identifier and transferred on request. Transactions carry no proof of work, so producing them costs nothing and their number cannot be bounded in advance. Every rule here that limits, delays or randomises exists because of that asymmetry; [30-security.md](30-security.md) sections 4 and 6 explain each.

A node without `[TXR]` MUST still parse `inv`, `tx`, `notfound` and `feefilter`, MUST NOT terminate on receiving them, and MUST observe `CON-46`.

## 1. Identifier negotiation

A transaction has two identifiers ([01-encoding.md](01-encoding.md) section 4). Witness data can be altered by a third party without invalidating a transaction, so a txid does not determine the bytes that will arrive; a wtxid does.

| ID | Rule | Reaction |
| --- | --- | --- |
| `TXR-01` | MUST send `wtxidrelay` during negotiation when the common version is at least 70016. | safety |
| `TXR-02` | When both sides sent `wtxidrelay`, MUST announce with `MSG_WTX` and MUST NOT announce with `MSG_TX`. Otherwise MUST announce with `MSG_TX`. | ignore (the announcement is dropped and the transaction never requested) |
| `TXR-03` | MUST ignore an announcement whose type does not match the negotiated identifier. | — |

## 2. Announcing

| ID | Rule | Reaction |
| --- | --- | --- |
| `TXR-06` | MUST accumulate announcements and transmit them in batches on a randomised, exponentially distributed schedule, rather than as each transaction is accepted. | safety |
| `TXR-07` | MUST use a longer average batching interval for connections it accepted than for connections it initiated, and MUST use one shared schedule for all accepted connections so that announcements to them occur at the same instants. | safety |

Peers track at most a few thousand outstanding announcements per connection and silently discard the excess; a node announces at a bounded rate. Transactions the node originated are re-announced periodically until a peer requests them.

## 3. Fee filtering

`feefilter` asks the peer not to announce transactions paying less than the stated rate.

| ID | Rule | Reaction |
| --- | --- | --- |
| `TXR-12` | With `[TXR]`, SHOULD send `feefilter` to each peer when the common version is at least 70013, and again when its threshold changes materially. | — |
| `TXR-13` | MUST ignore a `feefilter` whose value is negative or exceeds the total money supply. | — |
| `TXR-15` | MUST NOT terminate or record a peer that announces or sends a transaction below the rate it advertised. The filter is advisory. | — |
| `TXR-16` | MUST NOT send `feefilter` on a connection in the block-relay-only role. | ignore |

## 4. Requesting

| ID | Rule | Reaction |
| --- | --- | --- |
| `TXR-17` | MUST request a transaction announced by wtxid with `MSG_WTX`, and one announced by txid with `MSG_WITNESS_TX` when the peer advertises `NODE_WITNESS`. | deprioritise (a witness-stripped transaction cannot be relayed) |
| `TXR-18` | MUST hold at most one outstanding request for a given transaction across all peers. | safety |
| `TXR-19` | MUST delay a request by about two seconds when the announcing peer is one it did not itself connect to, again when the announcement was by txid while wtxid-announcing peers exist, and again when the peer already has many requests outstanding. Among equally eligible peers it chooses at random. | safety |
| `TXR-21` | MUST bound the number of announcements it tracks per peer, on the order of a few thousand, and MUST discard further announcements rather than terminate. | safety |
| `TXR-22` | MUST abandon a request unanswered for about a minute and then request from another announcer. | safety |

A `getdata` carries far fewer than the 50,000-vector limit, on the order of a thousand.

## 5. Serving

| ID | Rule | Reaction |
| --- | --- | --- |
| `TXR-25` | MUST answer a `getdata` for a transaction with `tx`, or by naming the inventory vector in a `notfound`. | deprioritise (the requester waits out its timeout) |
| `TXR-27` | MUST NOT serve a transaction that was not yet in its memory pool when it last sent announcements to the requesting peer, unless the transaction is in a block it recently relayed. | safety |
| `TXR-28` | MUST treat `notfound` as releasing its outstanding request, so that another announcer may be tried at once. | — |

Block inventory in a `notfound` is ignored (`BLK-25`).

## 6. Receiving

| ID | Rule | Reaction |
| --- | --- | --- |
| `TXR-30` | MUST NOT terminate or record a peer because a transaction it sent was invalid, non-standard, conflicting or below a threshold. Implementations that track the reference implementation at release 29 or earlier still punish consensus-invalid transactions; the punishment was removed in 2025. | — |
| `TXR-31` | MUST accept an unsolicited `tx` without terminating, and MAY discard it. | — |

A node remembers transactions it rejected and does not request them again until its tip changes, and does not process announcements while still synchronising its chain.

## 7. Missing ancestors

A transaction may arrive whose inputs reference transactions the node does not have. A node MAY retain it briefly and request the missing ancestors, bounding both the number retained and the time.

| ID | Rule | Reaction |
| --- | --- | --- |
| `TXR-35` | MUST request a missing ancestor by txid with `MSG_WITNESS_TX`, since only the txid is available from the spending transaction. | — |
| `TXR-36` | MUST answer a `getdata` of type `MSG_WITNESS_TX` for a transaction that `TXR-27` permits it to serve, even on a connection where announcements are by wtxid and the transaction was never announced by txid. | deprioritise (peers stall resolving ancestors) |

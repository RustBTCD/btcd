# 10. Address gossip

Capability tag: `[ADR]`.

Address gossip is how a node learns of peers it can connect to. Every address record is an unverified claim from an unidentified source, relayed by unaccountable intermediaries. The rules here bound what an adversary can achieve by exploiting that; [30-security.md](30-security.md) section 2 explains each.

A node without `[ADR]` MUST still parse `addr`, `addrv2` and `getaddr`, MUST NOT terminate on receiving them, and MUST NOT store or relay what they carry.

## 1. Choosing the message form

| ID | Rule | Reaction |
| --- | --- | --- |
| `ADR-02` | MUST accept `addrv2` from any peer, whether or not this node sent `sendaddrv2`. | — |
| `ADR-03` | MUST send `addrv2` to a peer that sent `sendaddrv2`, and `addr` to one that did not, omitting from `addr` any record whose address type has no version 1 encoding. | discard (`addrv2` to a peer that did not signal); safety (encoding a Tor, I2P or CJDNS address as zeros pollutes the network) |
| `ADR-04` | MUST send `sendaddrv2` during negotiation when the common version is at least 70016. | safety (without it the node learns no address outside IPv4 and IPv6) |

## 2. Rate limits

| ID | Rule | Reaction |
| --- | --- | --- |
| `ADR-06` | MUST limit the rate at which it processes records from each peer to an average of one per ten seconds, with an allowance of 1000 records granted on the first address message and a further 1000 whenever it sends `getaddr` to that peer. MUST shuffle the records of a message before discarding the excess, and MUST NOT terminate for the excess. | safety |
| `ADR-08` | MUST assume its peers apply the limit in `ADR-06` and pace its own transmission accordingly. | discard (silently) |

## 3. Announcing this node's address

| ID | Rule | Reaction |
| --- | --- | --- |
| `ADR-09` | With `[LSN]`, SHOULD announce its own address to each address-relay peer at an average interval of 24 hours, exponentially distributed. | — |
| `ADR-14` | MUST NOT announce an address for itself unless it has `[LSN]` and has corroborated the address from several peers it did not dial (`CON-23`). | safety |
| `ADR-10` | SHOULD send its first self-announcement on a connection as a message containing that record alone. Older implementations start the processing allowance at one record, and a self-announcement inside a larger message is then discarded. | — |
| `ADR-11` | MUST carry the current time and the service flags it actually offers in a self-announcement. | deprioritise (a stale announcement is stored but not relayed, `ADR-18`) |
| `ADR-13` | With `[PRV]`, MUST NOT announce its privacy-network address to peers reached over a public network, nor its public address to peers reached over a privacy network. | safety |

## 4. Relaying records

| ID | Rule | Reaction |
| --- | --- | --- |
| `ADR-15` | MUST NOT relay a record back to the peer it was received from. | safety |
| `ADR-16` | MUST relay a record to at most two peers, chosen by a function of the record, a per-node secret and a coarse time interval, so that the choice is stable for about 24 hours and not predictable by an observer. | safety |
| `ADR-18` | MUST relay a record only when all hold: the message carrying it had at most ten records; its timestamp is within the last ten minutes; the address is routable and of a type this node can relay; this node is not awaiting the peer's response to its own `getaddr`. Records failing a condition MAY still be stored. | safety |

## 5. Accepting records

| ID | Rule | Reaction |
| --- | --- | --- |
| `ADR-22` | MUST replace the timestamp of a record that is more than ten minutes in the future, or implausibly old, with a value that ranks it as old. | safety |
| `ADR-24` | MUST remember which addresses each peer has sent it and MUST NOT send those addresses back to that peer, whether or not they were stored. | safety |
| `ADR-25` | MUST bound the share of its address store that records learned from any single peer, and from any single net group, can occupy. | safety |

`ENC-18` governs which addresses may be stored at all.

## 6. Soliciting and answering `getaddr`

Responses are delivered on the responder's own address-sending schedule (`ADR-34`), not immediately.

| ID | Rule | Reaction |
| --- | --- | --- |
| `ADR-30` | MUST send `getaddr` only on connections it initiated, at most once per connection. MUST answer `getaddr` only on connections it accepted, at most once per connection, ignoring every other request. | ignore |
| `ADR-29` | MUST tolerate a delay of tens of seconds before a `getaddr` response arrives, and MUST NOT re-send the request. | — |
| `ADR-32` | MUST limit a response to at most 1000 records and to a small fraction of its store, selected at random. | safety |
| `ADR-33` | MUST return the same set of records to every requester arriving over the same network for an extended, randomised period on the order of a day, derived separately per network on which it accepts connections. | safety |
| `ADR-34` | MUST transmit address records on a randomised schedule averaging about 30 seconds per peer, batching what accumulates, rather than as each record is decided. | safety |

## 7. Block-relay-only connections

| ID | Rule | Reaction |
| --- | --- | --- |
| `ADR-36` | On a connection in the block-relay-only role, MUST NOT send `addr`, `addrv2` or `getaddr`, and MUST ignore any of them received. | ignore |

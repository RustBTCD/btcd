# 91. Conformance

## 1. Claiming conformance

An implementation conforms with respect to a set of capabilities when it satisfies every MUST-level rule binding those capabilities, both wire and safety, and advertises no service flag whose capability it does not provide (`CON-01`). SHOULD-level rules do not affect conformance.

An implementation MAY conform for a subset of capabilities. There is no minimum beyond the base set.

## 2. Rule index

### Base, binding on every node

| Module | Rules |
| --- | --- |
| Conventions | `MSG-01` to `MSG-03`, `CON-01` |
| Encoding | `ENC-01` to `ENC-06`, `ENC-08`, `ENC-09`, `ENC-11` to `ENC-15`, `ENC-18`, `ENC-20`, `ENC-21`, `ENC-23`, `ENC-25`, `ENC-26` |
| Plaintext transport | `T1-01` to `T1-10`, `T1-12`, `T1-15`, `T1-16` |
| Connection | `CON-10` to `CON-21`, `CON-23` to `CON-26`, `CON-28` to `CON-32`, `CON-34` to `CON-42`, `CON-44` to `CON-46`, `CON-48` to `CON-50` |
| Messages | `MSG-10`, `MSG-12` to `MSG-18`, `MSG-20`, `MSG-23` to `MSG-25` |
| Block relay | `BLK-01` to `BLK-29` |
| Peer management | `SEL-01`, `SEL-04`, `SEL-09` to `SEL-13`, `SEL-22` to `SEL-24` |
| Bootstrap | `BST-01`, `BST-02`, `BST-07`, `BST-14`, `BST-15` |

A node without `[ADR]` MUST additionally parse `addr`, `addrv2` and `getaddr` without terminating, and MUST NOT store or relay their contents. A node without `[TXR]` MUST additionally observe `CON-46` and tolerate `inv`, `tx`, `notfound` and `feefilter` without terminating.

### Per capability

| Capability | Adds |
| --- | --- |
| `[LSN]` | `SEL-17` to `SEL-21`. With `[ADR]`: `ADR-09` to `ADR-14`, `ADR-30` to `ADR-34`. Note `CON-49`: without `[BSA]` or `[BSL]` the node keeps no automatic inbound connection. |
| `[ADR]` | `ADR-02` to `ADR-36`. |
| `[BSA]` | `BLK-30` to `BLK-36`, serving the whole chain. Advertise `NODE_NETWORK` and `NODE_WITNESS`. |
| `[BSL]` | `BLK-30` to `BLK-36`, serving at least 288 blocks. Advertise `NODE_NETWORK_LIMITED` and `NODE_WITNESS`. |
| `[TXR]` | `TXR-01` to `TXR-36`. |
| `[CMP]` | `CMP-01` to `CMP-29`. |
| `[ENC]` | `T2-01` to `T2-30`. Advertise `NODE_P2P_V2`. |
| `[PRV]` | `ADR-13`, `BST-10`. |

## 3. Implementation order

Each stage is independently testable against a live peer.

1. **Connect.** Encoding, plaintext framing, `version` and `verack`, `ping` and `pong`, the deadlines of `CON-40` and `CON-41`. The node can now hold a connection indefinitely.
2. **Follow the chain.** `getheaders`, `headers`, `inv` for blocks, `getdata`, `block`. `BLK-01` to `BLK-29`. Do not advertise any service flag yet.
3. **Find peers.** `getaddr`, `addr`, `addrv2`, `sendaddrv2`, `SEL-*`, `BST-*`. The node no longer needs configured peers.
4. **Be useful.** `[LSN]` and one of `[BSA]` or `[BSL]`, with the corresponding flags. Advertising is now a commitment (`CON-01`, `BLK-31`).
5. **Relay transactions.** `[TXR]`, including the timing and scheduling rules of [22-transaction-relay.md](22-transaction-relay.md) sections 2 and 4, where the privacy properties live.
6. **Reduce latency and exposure.** `[CMP]`, then `[ENC]`.

## 4. Interoperability checks

Behaviours worth verifying against an independent implementation, chosen because each is easy to get wrong and fails silently.

| Check | Failure mode if wrong |
| --- | --- |
| First 16 bytes of an outbound version 1 connection are the `version` header | Peer classifies the connection as encrypted; handshake times out (`T1-15`) |
| Negotiation messages sent strictly between `version` and `verack` | Terminate (`CON-28`) |
| `feature` sent only at common version 70017 or above, with a 4-to-80-byte identifier and nothing after the data | Terminate (`CON-50`) |
| `version` advertises `NODE_WITNESS` with `NODE_NETWORK` or `NODE_NETWORK_LIMITED` when accepting automatic connections | Every automatic peer disconnects right after the handshake (`CON-49`) |
| `user_agent` at most 256 bytes | `version` discarded; presents as a timeout (`CON-21`) |
| Nonce never echoed from another connection | Peer concludes self-connection and terminates (`CON-19`) |
| Messages before the peer's `verack` are ignored, not punished | Honest peers dropped for ordinary races (`CON-48`) |
| First self-announcement sent alone | Discarded by older peers' rate limiter (`ADR-10`) |
| `getaddr` only on initiated connections; answered once, only on accepted ones | Request ignored; response never arrives (`ADR-30`) |
| Blocks requested with the witness flag | Witness-stripped block; identifiers do not match (`BLK-20`) |
| `headers` continuous, at most 2000, valid proof of work | Terminate and record (`BLK-03`, `BLK-04`, `ENC-08`) |
| Short `headers` only when no further headers exist | Peer stops synchronising (`BLK-07`) |
| `inv` for a block answered with `getheaders`, never `getdata` | Node downloads unvalidated data on request (`BLK-19`) |
| No transaction traffic toward a peer that sent `relay` false | Terminate (`CON-46`) |
| Announcement type matches the negotiated identifier | Announcements silently ignored (`TXR-02`) |
| Transaction `getdata` always answered with `tx` or `notfound` | Requester waits out its timeout (`TXR-25`) |
| Ancestor requests by txid answered on wtxid connections | Peers stall resolving ancestors (`TXR-36`) |
| Invalid transactions never punished | Network partitions along policy differences (`TXR-30`) |
| Unsupported encrypted handshake closed without sending a byte | Peer never retries with version 1 (`T2-26`) |

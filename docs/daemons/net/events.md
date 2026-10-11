# net events

Types are defined in [types.md](../../architecture/types.md).

Every event except `net.peer_connected` and `net.peer_disconnected` carries the decoded content of one P2P message (or the part of it routed to the subscriber). For a given peer, events are emitted in the order its messages arrived, after `net.peer_connected` and before `net.peer_disconnected`.

`InvItem` is `{"type": <string>, "hash": Hash}`, with `type` one of `block`, `witness_block`, `cmpct_block`, `tx`, `witness_tx`, `wtx`.

## `net.peer_connected`

The handshake with a peer completed (both `verack`s exchanged) and `hook.peer_connected` did not reject it.

| Field | Type | Description |
|---|---|---|
| `peer` | PeerId | |
| `addr` | string | `ip:port` |
| `inbound` | bool | |
| `conn_type` | string | `outbound_full_relay`, `manual`, `inbound` |
| `services` | Services | Services the peer advertised |
| `version` | number | Protocol version |
| `user_agent` | string | |
| `start_height` | Height | Height the peer reported |
| `relay` | bool | The peer wants transaction announcements (the `version` relay flag) |
| `wtxid_relay` | bool | Both sides sent `wtxidrelay` (BIP339) |

Subscribers: block-sync, tx-relay.

## `net.peer_disconnected`

| Field | Type | Description |
|---|---|---|
| `peer` | PeerId | |
| `reason` | string | Free text for logs, for example `timeout`, `misbehaving: <reason>`, `closed by peer` |

Last event about this peer. Subscribers: block-sync, tx-relay.

## `net.headers`

| Field | Type | Description |
|---|---|---|
| `peer` | PeerId | |
| `headers` | [HeaderHex] | In message order, at most 2000. May be empty. |

Subscriber: block-sync.

## `net.inv_blocks`

Block items of an `inv` message.

| Field | Type | Description |
|---|---|---|
| `peer` | PeerId | |
| `hashes` | [Hash] | |

Subscriber: block-sync.

## `net.block`

| Field | Type | Description |
|---|---|---|
| `peer` | PeerId | |
| `block` | BlockHex | |

Subscriber: block-sync.

## `net.getheaders`

| Field | Type | Description |
|---|---|---|
| `peer` | PeerId | |
| `locator` | [Hash] | At most 101 hashes, as Core's `MAX_LOCATOR_SZ` |
| `stop` | Hash or null | `null` when the message has an all-zero stop hash |

Subscriber: block-sync.

## `net.getdata_blocks`

Block items of a `getdata` message.

| Field | Type | Description |
|---|---|---|
| `peer` | PeerId | |
| `items` | [InvItem] | Types `block`, `witness_block`, `cmpct_block` |

Subscriber: block-sync.

## `net.sendheaders`

The peer prefers block announcements as `headers` (BIP130).

| Field | Type | Description |
|---|---|---|
| `peer` | PeerId | |

Subscriber: block-sync.

## `net.inv_txs`

Transaction items of an `inv` message.

| Field | Type | Description |
|---|---|---|
| `peer` | PeerId | |
| `items` | [InvItem] | Types `tx`, `wtx` |

Subscriber: tx-relay.

## `net.tx`

| Field | Type | Description |
|---|---|---|
| `peer` | PeerId | |
| `tx` | TxHex | |

Subscriber: tx-relay.

## `net.getdata_txs`

Transaction items of a `getdata` message.

| Field | Type | Description |
|---|---|---|
| `peer` | PeerId | |
| `items` | [InvItem] | Types `tx`, `witness_tx`, `wtx` |

Subscriber: tx-relay.

## `net.notfound_txs`

Transaction items of a `notfound` message.

| Field | Type | Description |
|---|---|---|
| `peer` | PeerId | |
| `items` | [InvItem] | |

Subscriber: tx-relay.

## `net.feefilter`

| Field | Type | Description |
|---|---|---|
| `peer` | PeerId | |
| `feerate` | FeeRate | The peer's minimum fee rate for announcements (BIP133) |

Subscriber: tx-relay.

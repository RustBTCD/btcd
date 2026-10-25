# net

The only daemon that talks to the internet. It turns peer connections into events and executes the other daemons'
`net.send` requests. Every other daemon sees peers only as `PeerId`s and decoded messages.

## Responsibilities

- Listen for inbound connections and open outbound ones (connection policy below).
- The v1 P2P transport: framing, magic, checksum, message decoding, and per-message size limits from rust-bitcoin's
  `bitcoin-p2p-messages`.
- The handshake: `version`, `verack`, `sendaddrv2`, `sendheaders` (`net` sends it after `verack`).
  A peer's `wtxidrelay` is accepted and ignored. Emits `hook.peer_connected` before accepting a peer and
  `net.peer_connected` after the handshake.
- Keep-alive: `ping`/`pong`, inactivity timeouts.
- Address management: DNS seeds, `addr`/`addrv2`/`getaddr`, the persisted address list.
- Turning inbound block and header messages into events for `block-sync`.
- Sending messages on behalf of other daemons (`net.send`).
- Peer discipline: disconnecting and discouraging misbehaving peers on request (`net.misbehaving`) and for protocol
  violations it detects itself.
- Backpressure towards peers: per-peer send buffer limits, and pausing all reads on `bus.pause`.

## Not responsibilities

- Deciding what to request from peers, or what to announce to them (`block-sync`).
- Transaction relay (not in v0.1).
- Judging block or transaction validity. `net` only checks that messages decode and respect size limits.

## Inbound message handling

| Message | Handling |
|---|---|
| `version`, `verack`, `wtxidrelay`, `sendaddrv2` | Handshake, internal. |
| `ping`, `pong` | Keep-alive, internal. |
| `addr`, `addrv2`, `getaddr` | Address management, internal. |
| `sendheaders` | Event `net.sendheaders`. |
| `inv` | Block items → `net.inv_blocks`. Any transaction item → disconnect: our `version` has `relay = false` (Core's `-blocksonly`). |
| `getdata` | Block items → `net.getdata_blocks`; transaction items are answered by `net` with one `notfound`. `MSG_FILTERED_BLOCK` → disconnect (`NODE_BLOOM` is not offered). |
| `headers` | Event `net.headers`. |
| `block` | Event `net.block`. |
| `tx` | Disconnect: our `version` has `relay = false` (Core's `-blocksonly`). |
| `getheaders` | Event `net.getheaders`. |
| `mempool`, `filterload`, `filteradd`, `filterclear`, `getcfilters`, `getcfheaders`, `getcfcheckpt` | Disconnect: requests a service that is not advertised, as Core does. |
| `notfound`, `feefilter`, `getblocks`, `sendcmpct`, `cmpctblock`, `getblocktxn`, `blocktxn`, `sendtxrcncl`, unknown commands | Ignored in v0.1. |

Any message other than `version` before the handshake completes, a message that fails to decode, or a message over its
size limit disconnects the peer and discourages its address. Transaction messages disconnect without discouraging, as in
Core.

Within one peer, events are emitted in the order its messages arrived, after `net.peer_connected` and before
`net.peer_disconnected`.

## Our `version` message

| Field | Value |
|---|---|
| version | 70016 |
| services | `NODE_NETWORK`, `NODE_WITNESS`, `NODE_NETWORK_LIMITED` |
| start height | height from the last `chain.tip_changed`, `0` before the first one |
| relay | `false`: v0.1 relays no transactions |
| user agent | `/rbtc:0.1.0/` plus the configured suffix |
| nonce | random per connection, used to detect connections to ourselves |

Peers with a protocol version below 31800 (Core's `MIN_PEER_PROTO_VERSION`) are rejected. Automatic outbound peers must
offer `NODE_NETWORK` and `NODE_WITNESS`.

## Connection policy

- Up to `max_outbound` (8) automatic full-relay outbound connections, chosen from the address list.
  No two automatic outbound peers share a network group (/16 for IPv4, /32 for IPv6).
- Manual peers from `add_peers` and `net.connect` come on top and are reconnected when lost.
  If `connect` is non-empty, only those peers are used and nothing is automatic (Core's `-connect`).
- Inbound connections are accepted while the total is below `max_connections`. When full, new inbound connections are
  refused (no eviction in v0.1).
- Discouraged addresses are refused inbound and never chosen outbound for `discourage_hours`.
- IPv4 and IPv6 only.

## Address management

- Seeds: the network's DNS seeds when the list is empty or every candidate fails; none on regtest.
- After the handshake with an outbound peer, `net` sends `getaddr` once.
- Received `addr`/`addrv2` records are rate-limited per peer, as Core does, and stored. Records for networks other than
  IPv4/IPv6 are dropped.
- An inbound peer's `getaddr` is answered once per connection with up to 1000 random known addresses.
- Received addresses are not forwarded to other peers in v0.1.
- The list holds at most 20 000 addresses: address, services, last seen, last success, attempts.
  It is saved to the `addresses` store every 15 minutes and at shutdown.

## Backpressure

- Each peer has a send queue. When it exceeds `send_buffer_kib`, `net` stops reading from that peer until the queue
  drains (Core's send buffer limit).
- On `bus.pause`, `net` stops reading from every peer and stops accepting connections until `bus.resume`.
  It keeps writing, handling `net.send`, and answering RPCs.

## Timeouts

| What | Value |
|---|---|
| Handshake | 60 s from TCP connect |
| Ping interval | 2 min |
| No message received | 20 min, then disconnect |
| Outbound TCP connect | 5 s |

## State

| State | Persistent |
|---|---|
| Connections: id, address, direction, negotiated features, counters, send queue | no |
| Address list | yes, store `addresses` ([storage.md](../../architecture/storage.md#net)) |
| Discouraged addresses with expiry | no |
| Last known tip height | no |

## Manifest

| | |
|---|---|
| source | yes |
| subscribes | `chain.tip_changed` |
| emits events | [events.md](events.md) |
| emits hooks | `peer_connected` |
| serves RPCs | [rpc.md](rpc.md) |
| calls | — |

## Concurrency

Async (tokio): one task per connection, one task for the connection manager and address list.
RPC requests are handled concurrently; `net.send` for one peer is applied in arrival order.

## Configuration

```toml
[net]
listen = true
bind = []                    # default: all interfaces on the network's port
max_connections = 125
max_outbound = 8
connect = []                 # "host:port"; if non-empty, the only peers used
add_peers = []               # "host:port"; manual peers in addition to automatic ones
dns_seeds = true
user_agent_suffix = ""
send_buffer_kib = 5000
discourage_hours = 24
```

## Not in v0.1

- Transaction relay, planned together with a mempool.
- BIP324 v2 transport.
- Inbound eviction, address bucketing, address forwarding.
- Block-relay-only and feeler connections.
- Tor, I2P, CJDNS.

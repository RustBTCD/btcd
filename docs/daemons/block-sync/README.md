# block-sync

The network side of blocks: it decides which headers and blocks to ask which peer for, hands what arrives to `chain`, serves headers and blocks to peers, and announces new blocks. It has no persistent state: block data comes from `chain`, peer data from `net` events.

## Responsibilities

- Header synchronisation, during IBD and afterwards.
- Block download: scheduling, the in-flight limits, timeouts and stall detection.
- Passing received headers and blocks to `chain` and reacting to the result (continue, re-request, punish the peer).
- Serving `getheaders` and `getdata` for blocks.
- Announcing new tips to peers with `headers` (BIP130) or `inv`.

## Not responsibilities

- Validating anything (`chain`, `utxo`). block-sync only acts on their verdicts.
- Connections and wire encoding (`net`).

## Per-peer state

Created on `net.peer_connected`, dropped on `net.peer_disconnected`.

| Field | Source |
|---|---|
| services, inbound, connection type, start height | `net.peer_connected` |
| prefers headers announcements | `net.sendheaders` |
| best known block (hash, height) | the last header of `net.headers` that `chain` accepted, an `inv` for a block `chain` knows, or a block announced to the peer; `start_height` until then |
| blocks in flight, with request time | our `getdata` requests |
| consecutive unconnected headers messages | `chain.submit_headers` results |
| queue of `getdata` items to serve | `net.getdata_blocks` |

## Header synchronisation

**During IBD** (the `ibd` flag from `chain.info` during initialisation, then from each `chain.tip_changed`):

1. Pick one sync peer among outbound peers that offer `NODE_NETWORK`. Send `getheaders` with `chain.locator`.
2. On `net.headers` from the sync peer, call `chain.submit_headers`. If the message had 2000 headers, ask for more with a locator starting at the last one. With fewer, the sync peer has nothing more; header sync is complete until a new announcement.
3. If the sync peer makes no progress for `headers_timeout_secs`, disconnect it (`net.disconnect`) and pick another.
4. Headers and block `inv`s from inbound peers are ignored during IBD.

**After IBD:** send `getheaders` with `chain.locator` once to every newly connected peer, and process announcements from every peer.

**Announcements and unconnected headers** (any peer, any time, subject to the IBD rule above):

- `net.headers`: `chain.submit_headers`. On `unconnected`, send `getheaders` with a fresh locator to that peer. Ten consecutive unconnected messages from one peer → `net.misbehaving`, as Core's `MAX_NUM_UNCONNECTING_HEADERS_MSGS`.
- `net.inv_blocks` naming a block `chain` does not know: send `getheaders` with `chain.locator` to that peer. The headers that come back lead to the download.
- `invalid` from `chain.submit_headers` → `net.misbehaving` with the reason.

## Block download

Scheduling runs after every `chain.submit_headers` or `chain.submit_block` result, on `chain.tip_changed`, on `net.peer_connected` and `net.peer_disconnected`, and on a one-second timer.

1. `chain.missing_blocks` with the configured window gives the blocks to fetch, in height order.
2. Skip blocks already in flight.
3. Assign each remaining block to a peer that has `NODE_NETWORK` and `NODE_WITNESS`, whose best known height is at least the block's height, and that has fewer than `max_blocks_in_flight_per_peer` blocks in flight. Prefer the peer with the fewest blocks in flight.
4. Send one `getdata` per peer with all its new assignments, type `witness_block`.

When a block arrives (`net.block`):

1. It counts as requested if it was in flight from that peer; remove it from in-flight.
2. Call `chain.submit_block` with `requested` set accordingly.
3. On `invalid` → `net.misbehaving`. If `data.mutated` is true, the block remains missing and is scheduled again for another peer.
4. On `unconnected` → `getheaders` to that peer.
5. Update the peer's best known block on `connected`, `stored` or `duplicate`.

Calls to `chain.submit_block` are made one at a time, in arrival order.

Timeouts, as Core:

- **Download timeout:** a block in flight for more than `block_timeout_secs` → disconnect the peer (`net.disconnect`) and fetch the block elsewhere.
- **Stalling:** when no block can be requested because the window is exhausted, and the block at the start of the window has been in flight longer than the stall timeout, disconnect the peer holding it. The stall timeout starts at `stall_timeout_secs`, doubles every time a peer is disconnected for stalling (up to 64 s), and shrinks back gradually after blocks arrive in time (Core's `BLOCK_STALLING_TIMEOUT_DEFAULT` and `BLOCK_STALLING_TIMEOUT_MAX`).
- A disconnected peer's blocks in flight become missing again and are rescheduled.

Timeouts disconnect but do not discourage the peer.

## Serving peers

- `net.getheaders`: ignored during IBD, as Core does. Otherwise `chain.headers_after` with the locator and stop hash, then `net.send` a `headers` message.
- `net.getdata_blocks`: items are queued per peer and served one at a time in order. For each: `chain.block` with verbosity 0; if the block is on the active chain, `net.send` a `block` message with `witness: false` for `block` items and `true` for `witness_block` and `cmpct_block` items. Blocks not on the active chain, or not stored, are skipped silently, as Core does. A peer's queue is dropped on disconnect.

Serving runs concurrently with block download, so a long `chain.submit_block` does not delay it.

## Announcing new blocks

On `chain.tip_changed` with `ibd: false` and a non-empty `connected` list, for each peer whose best known block is not already the new tip:

- If the peer prefers headers and at most 8 blocks were connected (Core's `MAX_BLOCKS_TO_ANNOUNCE`), `net.send` a `headers` message with the headers from the event.
- Otherwise `net.send` an `inv` with the new tip's hash.

The peer's best known block becomes the new tip.

## Manifest

| | |
|---|---|
| source | no |
| subscribes | `net.peer_connected`, `net.peer_disconnected`, `net.headers`, `net.inv_blocks`, `net.block`, `net.getheaders`, `net.getdata_blocks`, `net.sendheaders`, `chain.tip_changed` |
| emits events | — |
| emits hooks | — |
| serves RPCs | [rpc.md](rpc.md) |
| calls | `chain.submit_headers`, `chain.submit_block`, `chain.locator`, `chain.missing_blocks`, `chain.headers_after`, `chain.block`, `chain.info`, `net.send`, `net.disconnect`, `net.misbehaving` |

## Configuration

```toml
[block-sync]
max_blocks_in_flight_per_peer = 16
download_window = 1024
block_timeout_secs = 600
stall_timeout_secs = 2
headers_timeout_secs = 300
```

## Not in v0.1

- Compact blocks (BIP152).
- Parallel header download.

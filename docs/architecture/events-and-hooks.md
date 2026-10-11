# Events and hooks

## Events

An event announces a fact that already happened.

- **Name:** `<emitter>.<event>`, with the event part in snake_case and past tense or a noun for received data (`chain.block_connected`, `net.headers`). Each daemon defines its own events in its manifest; there is no central list.
- **Delivery:** to every subscriber, each through its own queue and credit window ([bus-protocol.md](bus-protocol.md#flow-control)). One slow subscriber does not delay the others.
- **No response.** The emitter never learns who received the event or what they did with it.
- **Order:** per emitter and subscriber, events arrive in the order they were emitted. Nothing else is ordered.
- **Before `bus.start`:** no daemon may emit. After `bus.shutdown` has started, `rbtcd` drops events instead of delivering them.
- **Payload:** identifiers and small facts. Bulk data (blocks, transaction lists) is not carried in events and is fetched by RPC. Exception: `net`'s inbound P2P data events (`net.block`, `net.tx`, `net.headers`).

### Handling rules for subscribers

1. Handlers MUST be idempotent with respect to state they read via RPC. Because events and responses are not ordered relative to each other, a subscriber can see a fact both through an RPC result and through an event, in either order.
2. A handler MAY call RPCs and emit events. It MUST NOT assume that the emitter's state still matches the event when it handles it. Example: a `chain.tip_changed` handler that calls `chain.info` may see a newer tip.
3. A subscriber MUST tolerate events about objects it does not know, for example an event for a peer whose `net.peer_connected` it did not process.

### Event catalogue

| Event | Emitter | Subscribers in v0.1 | Payload summary |
|---|---|---|---|
| `net.peer_connected` | net | block-sync, tx-relay | peer id, address, direction, services, version, relay flags |
| `net.peer_disconnected` | net | block-sync, tx-relay | peer id, reason |
| `net.headers` | net | block-sync | peer, headers |
| `net.inv_blocks` | net | block-sync | peer, block hashes |
| `net.block` | net | block-sync | peer, block |
| `net.getheaders` | net | block-sync | peer, locator, stop hash |
| `net.getdata_blocks` | net | block-sync | peer, requested blocks |
| `net.sendheaders` | net | block-sync | peer |
| `net.inv_txs` | net | tx-relay | peer, announced txids/wtxids |
| `net.tx` | net | tx-relay | peer, transaction |
| `net.getdata_txs` | net | tx-relay | peer, requested transactions |
| `net.notfound_txs` | net | tx-relay | peer, transactions the peer does not have |
| `net.feefilter` | net | tx-relay | peer, minimum fee rate |
| `chain.block_connected` | chain | — | hash, height, prev |
| `chain.block_disconnected` | chain | — | hash, height |
| `chain.tip_changed` | chain | net, block-sync, tx-relay | new tip, blocks connected and disconnected, IBD flag |
| `tx-relay.tx_accepted` | tx-relay | — | txid, wtxid, fee, vsize, source |
| `tx-relay.tx_removed` | tx-relay | — | txid, reason |

`utxo` and `block-sync` emit no events in v0.1. Full payloads are in each daemon's `events.md`:
[net](../daemons/net/events.md), [chain](../daemons/chain/events.md), [tx-relay](../daemons/tx-relay/events.md).

## Hooks

A hook is a synchronous interception point. The emitter asks "may this happen?" and waits for the answer.

### Semantics

- The hook table below is fixed in `rbtcd`. A daemon cannot invent hooks.
- Each hook has exactly one allowed emitter. The emitter sends a request `hook.<name>` and waits for the response.
- `rbtcd` calls the subscribers **one after another**, in the order the daemons appear in the config file. Each subscriber answers either:
  - `{"result": "continue"}`, or
  - `{"result": "reject", "reason": "<text>"}`.
- The first `reject` ends the chain. Later subscribers are not called.
- The emitter receives:
  - `{"result": "continue"}` if every subscriber continued, or there are none;
  - `{"result": "reject", "reason": "<text>", "by": "<daemon>"}` otherwise.
- If a subscriber does not answer within the hook's timeout, or answers with an error, `rbtcd` logs it and applies the hook's **default** for that subscriber, then continues with the next one.
- Hook payloads are read-only. A subscriber cannot modify what it is asked about.
- Hook edges are part of the dependency graph, so a hook subscriber cannot call an RPC of the hook's emitter ([manifest.md](manifest.md#validation)).

### Use

Hooks are policy decision points: whether an action may happen. State is never synchronised between daemons through hooks. Every flow in this specification is complete when no daemon subscribes to any hook.

### Hook table

| Hook | Emitter | When | Payload | Default | Timeout |
|---|---|---|---|---|---|
| `peer_connected` | net | After the peer's `version` message is accepted by `net`'s own checks, before `net` sends `verack`. | `{peer, addr, inbound, services, version, user_agent, start_height}` | `continue` | 5 s |
| `tx_accept` | tx-relay | After a transaction passed consensus and tx-relay's policy checks, before it enters the relay cache. Applies to peer and operator transactions. | `{txid, wtxid, tx, fee, vsize, peer}` (`peer` is `null` for operator submissions) | `continue` | 5 s |

On `reject`:

- `peer_connected`: `net` disconnects the peer without sending `verack` and logs the reason. No `net.peer_connected` or `net.peer_disconnected` event is emitted for that peer.
- `tx_accept`: tx-relay does not add the transaction, remembers its wtxid as rejected, and returns the reason to an operator submission.

No v0.1 daemon subscribes to either hook.

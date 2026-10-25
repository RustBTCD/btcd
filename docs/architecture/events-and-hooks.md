# Events and hooks

## Events

An event announces a fact that already happened.

- **Name:** `<emitter>.<event>`, with the event part in snake_case and past tense or a noun for received data
  (`chain.block_connected`, `net.headers`). Each daemon defines its own events in its manifest;
  there is no central list.
- **Delivery:** to every subscriber, each through its own queue and credit window
  ([bus-protocol.md](bus-protocol.md#flow-control)). One slow subscriber does not delay the others.
- **No response.** The emitter never learns who received the event or what they did with it.
- **Order:** per emitter and subscriber, events arrive in the order they were emitted. Nothing else is ordered.
- **Before `bus.start`:** no daemon may emit. After `bus.shutdown` has started, `rbtcd` drops events instead of
  delivering them.
- **Payload:** identifiers and small facts. Bulk data (blocks, transaction lists) is not carried in events and is
  fetched by RPC. Exception: `net`'s inbound P2P data events (`net.block`, `net.headers`).

### Handling rules for subscribers

1. Handlers MUST be idempotent with respect to state they read via RPC. Because events and responses are not ordered
   relative to each other, a subscriber can see a fact both through an RPC result and through an event, in either order.
2. A handler MAY call RPCs and emit events. It MUST NOT assume that the emitter's state still matches the event when it
   handles it. Example: a `chain.tip_changed` handler that calls `chain.info` may see a newer tip.
3. A subscriber MUST tolerate events about objects it does not know, for example an event for a peer whose
   `net.peer_connected` it did not process.

### Where events are defined

Each daemon lists its events, their subscribers and their schemas in its `events.md`:
[net](../daemons/net/events.md), [chain](../daemons/chain/events.md). `utxo` and `block-sync` emit no events in v0.1.

## Hooks

A hook is a synchronous interception point. The emitter asks "may this happen?" and waits for the answer.

### Semantics

- The hook table below is fixed in `rbtcd`. A daemon cannot invent hooks.
- Each hook has exactly one allowed emitter, named in the table. Manifests do not declare emitted hooks: `rbtcd` accepts
  `hook.<name>` only from the daemon the table names. The emitter sends the request and waits for the response.
- `rbtcd` calls the subscribers **one after another**, in the order the daemons appear in the config file.
  Each subscriber answers `continue`, or `reject` with a reason (`Answer` in [bus/hook.json](../schemas/bus/hook.json)).
- The first `reject` ends the chain. Later subscribers are not called.
- The emitter receives `continue` if every subscriber continued or there are none, otherwise the first `reject` with its
  reason and the rejecting daemon (`Outcome` in [bus/hook.json](../schemas/bus/hook.json)).
- If a subscriber does not answer within the hook's timeout, or answers with an error, `rbtcd` logs it and applies the
  hook's **default** for that subscriber, then continues with the next one.
- Hook payloads are read-only. A subscriber cannot modify what it is asked about.
- Hook edges are part of the dependency graph, so a hook subscriber cannot call an RPC of the hook's emitter
  ([manifest.md](manifest.md#validation)).

### Use

Hooks are policy decision points: whether an action may happen. State is never synchronised between daemons through
hooks. Every flow in this specification is complete when no daemon subscribes to any hook.

### Hook table

The hook table is the set of files in [bus/hooks/](../schemas/bus/hooks/). Each file names the hook's emitter, when it
is called, what a reject does, its default, its timeout and its payload. Every hook call returns the results in
[bus/hook.json](../schemas/bus/hook.json).

| Hook | Schema |
|---|---|
| `peer_connected` | [peer_connected.json](../schemas/bus/hooks/peer_connected.json) |

No v0.1 daemon subscribes to it.

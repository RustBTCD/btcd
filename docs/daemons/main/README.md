# rbtcd (main daemon)

The hub of the node. It contains no Bitcoin logic.

## Responsibilities

- Read the config file, own the data directory lock, create each daemon's data directory.
- Spawn the subdaemons, collect and validate their manifests, initialise them in dependency order, start them, shut them down in reverse order ([lifecycle.md](../../architecture/lifecycle.md)).
- Route events to subscribers, hooks through their subscriber chains, and RPCs to their owners ([bus-protocol.md](../../architecture/bus-protocol.md#message-classification)).
- Enforce the manifest: undeclared events, hooks and calls are protocol violations; RPC permissions are checked on every request.
- Run flow control: credit windows, waiting queues, pausing sources, the RPC in-flight limit ([bus-protocol.md](../../architecture/bus-protocol.md#flow-control)).
- Serve the operator socket ([rpc.md](../../architecture/rpc.md#operator-socket)).
- Collect the daemons' stderr into the node log.
- Stop the node when any daemon fails (fail-stop).

## Not responsibilities

- Interpreting payloads. Apart from its own `node.*` RPCs and `bus.*` messages, `rbtcd` reads only the envelope: `id`, `method`, and the frame size.
- Persisting anything except the lock file.
- Restarting failed daemons.

## State (in memory)

| State | Contents |
|---|---|
| Registry | Every manifest; derived tables: event → subscribers, hook → ordered subscribers, RPC → owner and permissions, topological order. |
| Connections | Per daemon: process id, lifecycle state (`spawned`, `registered`, `initialising`, `ready`, `running`, `stopping`, `stopped`), unacknowledged notification bytes, waiting queue. |
| Pending requests | Per forwarded request: original caller and id, target, and for hooks the position in the subscriber chain. |
| Pause state | Whether sources are currently paused. |

## Concurrency

One reader task and one writer task per connection, plus one routing task that owns the registry and the queues. Readers never block ([bus-protocol.md](../../architecture/bus-protocol.md#1-readers-never-block)).

## RPCs

Namespace `node`. Callable by the operator and by every daemon.

### `node.status`

Params: none.

```json
{
  "version": "0.1.0",
  "network": "main",
  "started_at": 1718000000,
  "state": "running",
  "paused": false,
  "daemons": [
    {"name": "utxo", "version": "0.1.0", "pid": 4242, "state": "running", "queued_bytes": 0, "unacked_bytes": 1024}
  ]
}
```

`state` is `starting`, `running` or `stopping`.

### `node.help`

| Param | Type | Required | Description |
|---|---|---|---|
| `daemon` | string | no | Limit to one daemon (`node` included). |
| `rpc` | string | no | Limit to one RPC; requires `daemon`. |

Returns a list of RPC descriptors as declared in the manifests ([manifest.md](../../architecture/manifest.md#schema)), filtered to what the caller may call. The `rbtc` CLI uses it to map arguments.

```json
{"rpcs": [{"name": "chain.block", "description": "…", "operator": true, "one_way": false, "params": [ … ]}]}
```

Errors: `method_not_found` if the daemon or RPC does not exist.

### `node.registry`

Params: none. Returns the whole registry for debugging: for each daemon its manifest; for each event its emitter and subscribers; for each hook its emitter and ordered subscribers; the dependency graph and the startup order.

### `node.stop`

Params: none. Operator only. Returns `{}` and then starts the orderly shutdown.

## Configuration

`[node]` and `[node.bus]`, `[node.paths]`; see [lifecycle.md](../../architecture/lifecycle.md#configuration).

## Hooks owned

The hook table lives in `rbtcd` ([events-and-hooks.md](../../architecture/events-and-hooks.md#hook-table)). `rbtcd` neither emits nor subscribes to hooks.

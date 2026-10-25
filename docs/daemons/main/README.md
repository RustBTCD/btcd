# rbtcd (main daemon)

The hub of the node. It contains no Bitcoin logic.

## Responsibilities

- Read the config file, own the data directory lock, create each daemon's data directory.
- Spawn the subdaemons, collect and validate their manifests, initialise them in dependency order, start them, shut them
  down in reverse order ([lifecycle.md](../../architecture/lifecycle.md)).
- Route events to subscribers, hooks through their subscriber chains, and RPCs to their owners
  ([bus-protocol.md](../../architecture/bus-protocol.md#message-classification)).
- Enforce the manifest: undeclared events, hooks and calls are protocol violations; RPC permissions are checked on every
  request.
- Run flow control: credit windows, waiting queues, pausing sources, the RPC in-flight limit
  ([bus-protocol.md](../../architecture/bus-protocol.md#flow-control)).
- Serve the operator socket ([rpc.md](../../architecture/rpc.md#operator-socket)).
- Collect the daemons' stderr into the node log.
- Stop the node when any daemon fails (fail-stop).

## Not responsibilities

- Interpreting payloads. Apart from its own `node.*` RPCs and `bus.*` messages, `rbtcd` reads only the envelope:
  `id`, `method`, and the frame size.
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

One reader task and one writer task per connection, plus one routing task that owns the registry and the queues.
Readers never block ([bus-protocol.md](../../architecture/bus-protocol.md#1-readers-never-block)).

## RPCs

Namespace `node`. Callable by the operator and by every daemon. Requests and responses are defined by the schemas in
[schemas/rpc/](schemas/rpc/); they use the manifest types in [bus/manifest.json](../../schemas/bus/manifest.json).

| RPC | Schema | Purpose |
|---|---|---|
| [`node.status`](#nodestatus) | [status.json](schemas/rpc/status.json) | Daemons, their state and pids, uptime, network |
| [`node.help`](#nodehelp) | [help.json](schemas/rpc/help.json) | RPC descriptions and parameters |
| [`node.registry`](#noderegistry) | [registry.json](schemas/rpc/registry.json) | The whole registry |
| [`node.stop`](#nodestop) | [stop.json](schemas/rpc/stop.json) | Orderly shutdown |

### `node.status`

The node's state (`starting`, `running`, `stopping`) and, for each daemon, its lifecycle state, pid and queued and
unacknowledged notification bytes. A daemon's `version` is `null` until its `bus.init`.

### `node.help`

The RPC descriptors from the manifests, grouped by daemon (`node` for rbtcd's own RPCs) and filtered to what the caller
may call. `daemon` and `rpc` narrow the answer. The `rbtc` CLI uses it to map arguments.

Errors: `method_not_found` if the daemon or RPC does not exist.

### `node.registry`

The whole registry, for debugging: every manifest; for each event its emitter and subscribers; for each hook its emitter
and its subscribers in calling order; the dependency graph; the startup order.

### `node.stop`

Operator only. Returns `{}` and then starts the orderly shutdown.

## Configuration

`[node]` and `[node.bus]`, `[node.paths]`; see [lifecycle.md](../../architecture/lifecycle.md#configuration).

## Hooks owned

The hook table lives in `rbtcd` ([events-and-hooks.md](../../architecture/events-and-hooks.md#hook-table)).
`rbtcd` neither emits nor subscribes to hooks.

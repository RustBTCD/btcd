# Manifest

The first message a subdaemon sends is the request `bus.init`. Its params are the manifest: everything the daemon will do on the bus. `rbtcd` builds its routing tables and permission checks from the manifests alone, so a daemon can never send or receive anything its manifest does not declare.

## Schema

```json
{
  "name": "chain",
  "version": "0.1.0",
  "protocol": 1,
  "source": false,
  "subscribes": {
    "events": ["net.peer_connected"],
    "hooks": []
  },
  "emits": {
    "events": [
      {"name": "block_connected", "description": "A block was connected to the active chain."}
    ],
    "hooks": []
  },
  "rpcs": [
    {
      "name": "block",
      "description": "Return a stored block.",
      "operator": true,
      "callers": ["block-sync"],
      "one_way": false,
      "params": [
        {"name": "block", "type": "string", "required": true, "description": "Block hash or height."},
        {"name": "verbosity", "type": "integer", "required": false, "default": 1, "description": "0: hex, 1: decoded."}
      ]
    }
  ],
  "calls": ["utxo.info", "utxo.connect_block", "utxo.disconnect_block"]
}
```

| Field | Type | Meaning |
|---|---|---|
| `name` | string | Daemon name. MUST equal the name `rbtcd` started it under. |
| `version` | string | The daemon's own version, shown by `rbtc node status`. |
| `protocol` | integer | Bus protocol version. v0.1 is `1`. |
| `source` | bool | The daemon takes input from outside the node and honours `bus.pause` ([bus-protocol.md](bus-protocol.md#3-pausing-sources)). |
| `subscribes.events` | [string] | Full names (`<emitter>.<event>`) of events to receive. |
| `subscribes.hooks` | [string] | Hook names (without the `hook.` prefix) to receive. |
| `emits.events` | [object] | Events this daemon emits: short `name` (main prefixes the daemon name) and `description`. |
| `emits.hooks` | [string] | Hooks this daemon emits. Each must name this daemon as its emitter in the hook table. |
| `rpcs` | [object] | RPCs this daemon serves. See below. |
| `calls` | [string] | Full names (`<daemon>.<rpc>`) of every RPC this daemon may call, including `node.*` RPCs. |

RPC descriptor:

| Field | Type | Meaning |
|---|---|---|
| `name` | string | Short name; the full name is `<daemon>.<name>`. |
| `description` | string | One line, shown by `rbtc help`. |
| `operator` | bool | Callable from the operator socket. |
| `callers` | [string] | Daemons allowed to call it. `["*"]` allows every daemon. Empty means operator-only (requires `operator: true`). |
| `one_way` | bool | Called as a notification and never answered ([rpc.md](rpc.md#one-way-rpcs)). One-way RPCs MUST have `operator: false`. |
| `params` | [object] | Ordered parameters: `name`, `type`, `required`, optional `default`, `description`. The order defines how the CLI maps positional arguments. |

Parameter types are the ones the CLI must be able to parse from a command line:

| Type | JSON | CLI input |
|---|---|---|
| `string` | string | taken as is |
| `integer` | number | decimal |
| `boolean` | `true`/`false` | `--name` alone means `true`; `--name=false` |
| `json` | any | parsed as JSON |

Semantic types (hashes, hex, amounts) are all `string` or `integer` on the bus; their formats are in [types.md](types.md).

## Init response

When the registry is valid and it is this daemon's turn to initialise ([lifecycle.md](lifecycle.md#startup)), `rbtcd` answers `bus.init` with:

```json
{
  "network": "main",
  "datadir": "/home/alice/.rbtc/main/chain",
  "config": { "assume_valid": "default" },
  "node_version": "0.1.0"
}
```

| Field | Meaning |
|---|---|
| `network` | `main`, `testnet4`, `signet` or `regtest`. |
| `datadir` | The daemon's own data directory, already created. The daemon MUST NOT write outside it. |
| `config` | The daemon's section of the config file, as JSON, unvalidated. The daemon applies defaults and MUST reject unknown keys and invalid values: it writes the error to stderr and exits with a non-zero code, which stops the node. |
| `node_version` | Version of `rbtcd`. |

If the registry is invalid, `rbtcd` answers with an error (code `invalid_manifest`, see [rpc.md](rpc.md#error-codes)) whose `data` lists every problem, and the node stops.

## Validation

`rbtcd` waits for all manifests, then checks the registry as a whole. All problems are reported together.

1. `protocol` is supported; `name` matches; no two daemons share a name; no daemon is named `node`, `bus` or `hook`.
2. Event and RPC short names match `[a-z][a-z0-9_]*` and are unique within the daemon across events and RPCs together, so a notification `net.x` cannot be both an event and a one-way RPC.
3. Each entry of `subscribes.events` is declared in another daemon's `emits.events`. A daemon cannot subscribe to its own events.
4. Each hook in `subscribes.hooks` and `emits.hooks` exists in the hook table, and each emitted hook names this daemon as its emitter.
5. Each entry of `calls` exists, is not one of the daemon's own RPCs, and permits this daemon in `callers` (or `"*"`). `node.*` RPCs are callable by every daemon.
6. RPC descriptors are consistent: one-way RPCs are not operator RPCs; an RPC with empty `callers` has `operator: true`; parameter names are unique; required parameters come before optional ones.
7. **The dependency graph is acyclic.** Edges: daemon A → daemon B if A lists an RPC of B in `calls`, or A emits a hook that B subscribes to.
8. Every daemon listed in the config file sent a manifest within `init_timeout`.

Declaring an event that nobody subscribes to is allowed.

## Example: the complete v0.1 registry

| Daemon | source | subscribes | emits | calls |
|---|---|---|---|---|
| net | yes | `chain.tip_changed` | events: [net/events.md](../daemons/net/events.md); hooks: `peer_connected` | — |
| chain | no | — | events: [chain/events.md](../daemons/chain/events.md) | `utxo.info`, `utxo.connect_block`, `utxo.disconnect_block` |
| utxo | no | — | — | — |
| block-sync | no | `net.peer_connected`, `net.peer_disconnected`, `net.headers`, `net.inv_blocks`, `net.block`, `net.getheaders`, `net.getdata_blocks`, `net.sendheaders`, `chain.tip_changed` | — | `chain.submit_headers`, `chain.submit_block`, `chain.locator`, `chain.missing_blocks`, `chain.headers_after`, `chain.block`, `chain.info`, `net.send`, `net.disconnect`, `net.misbehaving` |
| tx-relay | no | `net.peer_connected`, `net.peer_disconnected`, `net.inv_txs`, `net.tx`, `net.getdata_txs`, `net.notfound_txs`, `net.feefilter`, `chain.tip_changed` | events: [tx-relay/events.md](../daemons/tx-relay/events.md); hooks: `tx_accept` | `utxo.check_tx`, `utxo.unspent`, `chain.info`, `net.send`, `net.misbehaving` |

RPCs served by each daemon are listed in its `rpc.md`.

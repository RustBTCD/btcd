# Manifest

The first message a subdaemon sends is the request `bus.init`. Its params are the manifest:
everything the daemon will do on the bus. `rbtcd` builds its routing tables and permission checks from the manifests
alone, so a daemon can never send or receive anything its manifest does not declare.

A daemon's `rpcs` and `emits.events` are generated from its schemas ([schemas.md](schemas.md)), so they always
match the spec.

## Schema

```json
{
  "name": "chain",
  "version": "0.1.0",
  "protocol": 1,
  "subscribes": {
    "events": ["net.peer_connected"],
    "hooks": []
  },
  "emits": {
    "events": [
      {"name": "block_connected", "description": "A block was connected to the active chain."}
    ]
  },
  "rpcs": [
    {
      "name": "block",
      "description": "A stored block, decoded.",
      "operator": true,
      "params": [
        {"name": "block", "type": "string", "required": true, "description": "Block hash or height."}
      ]
    }
  ],
  "calls": ["utxo.info", "utxo.connect_block", "utxo.disconnect_block"]
}
```

The exact definition is `Manifest` in [bus/manifest.json](../schemas/bus/manifest.json), with a description of every
field. In short: the daemon's name and version, the bus protocol version (v0.1 is `1`), the events and hooks it
subscribes to, the events it emits, the RPCs it serves, and every RPC it calls (full `<daemon>.<rpc>` names, `node.*`
included). An RPC's `params` are ordered; the order defines how the CLI maps positional arguments.

Parameter types are the ones the CLI must be able to parse from a command line:

| Type | JSON | CLI input |
|---|---|---|
| `string` | string | taken as is |
| `integer` | number | decimal |
| `boolean` | `true`/`false` | `--name` alone means `true`; `--name=false` |
| `json` | any | parsed as JSON |

Semantic types (hashes, hex, amounts) are all `string` or `integer` on the bus; their formats are in
[types.md](types.md).

## Init response

When the registry is valid and it is this daemon's turn to initialise ([lifecycle.md](lifecycle.md#startup)), `rbtcd`
answers `bus.init` with:

```json
{
  "network": "main",
  "datadir": "/home/alice/.rbtc/main/chain",
  "config": { "assume_valid": "default" },
  "node_version": "0.1.0"
}
```

The exact definition is `InitResult` in [bus/control.json](../schemas/bus/control.json). `datadir` is the daemon's own
data directory, already created; the daemon MUST NOT write outside it. `config` is the daemon's section of the config
file, unvalidated: the daemon applies defaults and MUST reject unknown keys and invalid values by writing the error to
stderr and exiting with a non-zero code, which stops the node.

If the registry is invalid, `rbtcd` answers with an error (code `invalid_manifest`, see [rpc.md](rpc.md#error-codes))
whose `data` lists every problem, and the node stops.

## Validation

`rbtcd` waits for all manifests, then checks the registry as a whole. All problems are reported together.

1. `protocol` is supported; `name` matches; no two daemons share a name; no daemon is named `node`, `bus` or `hook`.
2. Event and RPC short names match `[a-z][a-z0-9_]*` and are unique within the daemon across events and RPCs together.
3. Each entry of `subscribes.events` is declared in another daemon's `emits.events`. A daemon cannot subscribe to its
   own events.
4. Each hook in `subscribes.hooks` exists in the hook table. Who emits a hook is fixed by the table, not declared
   in manifests.
5. Each entry of `calls` exists and is not one of the daemon's own RPCs. `node.*` RPCs are callable by every daemon.
6. RPC descriptors are consistent: parameter names are unique; required parameters come before optional ones.
7. **The dependency graph is acyclic.** Edges: daemon A → daemon B if A lists an RPC of B in `calls`, or A is the
   emitter of a hook (per the hook table) that B subscribes to. The startup order is computed from this graph
   ([lifecycle.md](lifecycle.md#startup)); daemons declare no order themselves.
8. Every daemon listed in the config file sent a manifest within `init_timeout`.

Declaring an event that nobody subscribes to is allowed.

## Example: the complete v0.1 registry

| Daemon | subscribes | emits | calls |
|---|---|---|---|
| net | `chain.tip_changed` | events: [net/events.md](../daemons/net/events.md); hook `peer_connected` (hook table) | — |
| chain | — | events: [chain/events.md](../daemons/chain/events.md) | `utxo.info`, `utxo.connect_block`, `utxo.disconnect_block` |
| utxo | — | — | — |
| block-sync | `net.peer_connected`, `net.peer_disconnected`, `net.headers`, `net.inv_blocks`, `net.block`, `net.getheaders`, `net.getdata_blocks`, `net.sendheaders`, `chain.tip_changed` | — | `chain.submit_headers`, `chain.submit_block`, `chain.locator`, `chain.missing_blocks`, `chain.headers_after`, `chain.raw_block`, `chain.info`, `net.send`, `net.disconnect`, `net.misbehaving` |

RPCs served by each daemon are listed in its `rpc.md`.

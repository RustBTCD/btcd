# Schemas and API crates

The shape of every RPC and event is specified as a JSON Schema file. The Rust API each daemon uses to call, serve,
emit and receive is generated from these files, so the spec and the code cannot disagree. Core Lightning works the
same way (`doc/schemas/` and its `msggen` generator).

`rbtcd` does not load schemas at runtime. They are used at build time, in tests, and for generated documentation.

## Files

```
docs/schemas/
├── common.json                     types shared by several daemons (Hash, Height, BlockRef, …)
└── bus/                            rbtcd's own contract
    ├── envelope.json               request, notification, response, error object
    ├── manifest.json               the manifest (bus.init params) and its parts
    ├── control.json                params and results of the bus.* messages
    ├── hook.json                   a subscriber's answer and the emitter's outcome of a hook call
    └── hooks/<hook>.json           one file per hook in the hook table
docs/daemons/<daemon>/schemas/      main/ holds rbtcd's node.* RPCs
├── types.json                      types shared by the daemon's own files
├── rpc/<rpc>.json                  one file per RPC the daemon serves
└── events/<event>.json             one file per event the daemon emits
```

An RPC file describes the RPC completely, including what the manifest and the CLI need. The CLI type of each
parameter (`string`, `integer`, …) is derived from its schema.

```json
{
  "rpc": "net.disconnect",
  "description": "Close a connection. A manual peer disconnected this way is not reconnected.",
  "operator": true,
  "request": {
    "type": "object",
    "additionalProperties": false,
    "required": ["peer"],
    "properties": {
      "peer": {"$ref": "../../../../schemas/common.json#/$defs/PeerId"},
      "reason": {"type": "string", "description": "Logged; default requested."}
    }
  },
  "response": {"type": "object", "additionalProperties": false},
  "examples": [{"request": {"peer": 7}, "response": {}}, {"request": {"peer": 7, "reason": "stalling"}, "response": {}}]
}
```

An event file has `event`, `description`, `payload` and `examples` in the same way. A hook file has `hook`,
`description` (when it is called and what a reject does), `emitter`, `default`, `timeout_secs`, `payload` and
`examples`; the result of every hook call is defined once, in `hook.json`. Type files hold `$defs`; a definition may
carry its own `examples`.

## Ownership

- A daemon owns the schemas of the RPCs it serves and the events it emits. No other daemon defines them.
- A daemon's schemas refer only to its own files and to `common.json`. A type used by several of a daemon's files goes
  in its `types.json`; a type needed by two daemons moves to `common.json`.
- Hooks belong to the hook table in `rbtcd`, so their schemas are under `bus/hooks/`.
- `rbtcd` owns `bus/` and `docs/daemons/main/schemas/`; its `node.*` RPCs may refer to `bus/` (for example to the
  manifest).
- [types.md](types.md) describes the common types in prose; `common.json` is their exact definition.

## Conventions

The conventions keep the schemas easy to read and the generated Rust idiomatic.

- JSON Schema draft 2020-12.
- Every object has `"additionalProperties": false`: a schema describes exactly what the current version sends.
  Generated code rejects unknown fields only in RPC requests; in responses and events it ignores them, so receivers
  keep working when a newer daemon adds fields ([types.md](types.md#rules)).
- Optional fields are omitted when absent; `null` is used only where a schema allows it explicitly.
- `oneOf` has two uses only:
  - a value with several variants: objects, each with a constant `type` field (as `PeerMessage` in
    [net/schemas/types.json](../daemons/net/schemas/types.json));
  - a nullable value: `{"oneOf": [<schema>, {"type": "null"}]}`.

  `anyOf` and `allOf` are not used.
- Integers carry their range: `"minimum": 0` for unsigned values, plus `"maximum": 4294967295` for 32-bit ones, so the
  generator picks the right Rust integer type.
- Shared types are always referenced with `$ref`, never copied.
- A value of any shape (a config section, the `params` of the envelope, error `data`) is a schema without `type`,
  only a `description`.
- Every RPC, event and hook file has at least one example.

## Generated code

```
crates/
├── rbtc-bus/              transport, RbtcRpc, the Rpc and Event traits, common types
├── rbtc-net-api/          generated from docs/daemons/net/schemas/
├── rbtc-chain-api/
├── rbtc-utxo-api/
└── rbtc-block-sync-api/
```

`rbtc-bus` is written by hand. Common types (`Hash`, `Height`, `BlockHex`, …) are hand-written Rust types with real
parsing and formatting; the generator maps `common.json` references to them.

Each `rbtc-<daemon>-api` crate is generated at build time from its daemon's schemas. Generated code is not committed.
It contains:

| Generated | Used by | Purpose |
|---|---|---|
| Structs and enums for requests, responses and event payloads | everyone | typed messages |
| An implementation of `Rpc` or `Event` per RPC and event | `rbtc-bus` | ties the bus name to its types, so `RbtcRpc::call` and `emit` work for any daemon |
| A client trait, for example `ChainRpc` | daemons that call the daemon | adds `rpc.chain().block(…)` to `RbtcRpc` |
| Typed `emit` helpers and event decoding | the emitter and its subscribers | send and receive events without manual JSON |
| A server trait, for example `ChainServer`, and its request dispatch | the daemon itself | the compiler checks that the daemon implements every RPC in its schemas |
| The daemon's `rpcs` and `emits.events` manifest entries | the daemon itself | the manifest always matches the schemas |

Calling another daemon means depending on its API crate:

```rust
let info = rpc.chain().block(BlockRequest { block: hash.into() }).await?;
```

A third-party daemon follows the same pattern: its own schemas and its own API crate, with no change to `rbtc-bus`.

Tests check the schemas against real traffic: every example parses into the generated types and serializes back to
the same JSON, and integration tests validate the messages on the bus against the schemas.

## Checking

`make check-schemas` validates every example against its schema and checks the conventions above, the file names and
the `$ref` rules. It runs [scripts/check-schemas.sh](../../scripts/check-schemas.sh), which needs `jq` and
`jsonschema-cli` (`cargo install jsonschema-cli`).

## Documentation

- `rpc.md` and `events.md` keep what a schema cannot say: semantics, errors, ordering, examples. Once a daemon's
  schemas exist, its field tables are replaced by links to them.
- Until a daemon's schemas are written, the field tables in its Markdown files are the spec.
- Web documentation for the RPCs and events will be generated from the schemas.

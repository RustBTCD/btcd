# rbtc v0.1 specification

rbtc is a Bitcoin full node built as a set of cooperating processes: one main daemon (`rbtcd`) and several subdaemons
that talk to each other only through it. Mining and wallet functionality are out of scope.

What each version must deliver: [milestones.md](milestones.md).

## Reading order

1. [architecture/overview.md](architecture/overview.md): processes, responsibilities, data ownership, communication
   graphs.
2. [architecture/bus-protocol.md](architecture/bus-protocol.md): how processes talk: transport, framing, routing,
   ordering, flow control.
3. [architecture/manifest.md](architecture/manifest.md): the init message every subdaemon sends, and how main validates
   it.
4. [architecture/events-and-hooks.md](architecture/events-and-hooks.md): event and hook semantics, the fixed hook table,
   the event catalogue.
5. [architecture/rpc.md](architecture/rpc.md): RPC semantics, permissions, operator socket, CLI, error codes, the RPC
   catalogue.
6. [architecture/lifecycle.md](architecture/lifecycle.md): startup, shutdown, failures, configuration, data directory.
7. [architecture/storage.md](architecture/storage.md): who owns which data and how state stays consistent across
   processes and crashes.
8. [architecture/types.md](architecture/types.md): JSON representation of the common types.
9. [flows/](flows/README.md): end-to-end flows as sequence diagrams.
10. [daemons/](daemons/): one folder per process with its responsibilities, state, manifest, events and RPCs.

## Layout

```
docs/
├── README.md                  this file
├── milestones.md              what each version must deliver
├── architecture/
│   ├── overview.md
│   ├── bus-protocol.md
│   ├── manifest.md
│   ├── events-and-hooks.md
│   ├── rpc.md
│   ├── lifecycle.md
│   ├── storage.md
│   └── types.md
├── flows/
│   ├── README.md
│   ├── startup-and-shutdown.md
│   ├── peer-lifecycle.md
│   ├── initial-sync.md
│   ├── block-processing.md
│   ├── new-block-relay.md
│   ├── reorg.md
│   └── operator-queries.md
└── daemons/
    ├── main/        README.md
    ├── net/         README.md  events.md  rpc.md
    ├── chain/       README.md  events.md  rpc.md
    ├── utxo/        README.md  rpc.md
    └── block-sync/  README.md  rpc.md
```

## Conventions

- **MUST / MUST NOT / SHOULD / MAY** are used in the RFC 2119 sense.
- `rbtcd` is the main daemon. "Subdaemon" or "daemon" is any of `net`, `chain`, `utxo`, `block-sync`.
  "Main" always means `rbtcd`.
- Event names are `<emitter>.<event>`, RPC names are `<daemon>.<rpc>`, hook names are `hook.<hook>`, bus control
  messages are `bus.<message>`.
- Daemon names are kebab-case (they appear on the command line). Event, RPC and field names are snake_case.
- Sequence diagrams show logical communication between daemons and omit `rbtcd`, which relays every message.
  [bus-protocol.md](architecture/bus-protocol.md#physical-path-of-a-message) shows the physical path once.
- Bitcoin Core functions are named where our behaviour must match them, for example `CheckBlock` or `ActivateBestChain`.
  BIPs are cited by number.

## Glossary

| Term | Meaning |
|---|---|
| Bus | The set of connections between `rbtcd` and the subdaemons, plus the routing rules in `rbtcd`. |
| Event | A fact that already happened, broadcast to all subscribers, no response. |
| Hook | A synchronous interception point from the fixed table in `rbtcd`. The emitter waits until every subscriber has answered `continue` or one has answered `reject`. |
| RPC | A command or query addressed to exactly one daemon, with a response. A one-way RPC has no response. |
| Manifest | What a subdaemon declares in its init message: subscriptions, emitted events and hooks, RPCs it serves, RPCs it calls. |
| Operator | A human or external program using the operator socket, normally through the `rbtc` CLI. |
| Active chain | The chain of blocks currently connected to the UTXO set, from genesis to the tip. |
| Best header | The tip of the most-work valid header chain known, which can be ahead of the active tip. |
| IBD | Initial block download: the node's tip is too old or has too little work to be considered synced (Core's `IsInitialBlockDownload`). |

# Flows

End-to-end behaviour of the node as sequence diagrams. Each flow names the daemons involved and links to the rules it
relies on; the daemon READMEs remain the authority.

| Flow | What it covers |
|---|---|
| [startup-and-shutdown.md](startup-and-shutdown.md) | Manifests, ordered initialisation, `bus.start`, orderly shutdown, crash recovery at startup |
| [peer-lifecycle.md](peer-lifecycle.md) | Connecting, the handshake and `hook.peer_connected`, misbehaviour, disconnection |
| [initial-sync.md](initial-sync.md) | Headers-first IBD: header sync, the download window, stalling |
| [block-processing.md](block-processing.md) | One block from `net` to the UTXO set: checks, storage, connection, invalid blocks |
| [new-block-relay.md](new-block-relay.md) | Steady state: a new block is announced, fetched, connected and announced onwards |
| [reorg.md](reorg.md) | Switching to a branch with more work, including a branch that turns out invalid |
| [operator-queries.md](operator-queries.md) | CLI requests: chain state, blocks, transactions, coins, peers |

## Conventions

- Diagrams show logical messages between daemons. Every arrow physically goes through `rbtcd`
  ([bus-protocol.md](../architecture/bus-protocol.md#physical-path-of-a-message)).
- `->>` with a reply `-->>` is an RPC. `-)` is an event, or an RPC whose response the caller
  does not wait for (the response is not drawn).
- Event names are written in full (`chain.tip_changed`); RPC names in full (`utxo.connect_block`).
- Peers are written `P1`, `P2`, and the P2P message they exchange with `net` is shown as `net` ↔ peer arrows.

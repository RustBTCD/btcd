# Milestones

A short list of what each version must deliver. The details are in the rest of `docs/`.

## v0.1

The first working node.

- `rbtcd` starts the daemons, and they talk to each other through it using manifests, events, hooks and RPCs.
- The daemons `net`, `chain`, `utxo` and `block-sync` work together.
- The node syncs the chain from peers, validates every block and relays new blocks, compatible with Bitcoin Core.
- The operator can control and query the node with the `rbtc` CLI.

Not the goal of v0.1:

- a mempool;
- transaction relay (the `tx-relay` daemon);
- compact blocks (BIP152).

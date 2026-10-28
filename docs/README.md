# rbtc v0.1 specification

rbtc is a Bitcoin full node. v0.1 is a single process made of separate modules that interact through direct calls and
in-memory queues.

v0.1:

- connects to peers and keeps the connections alive;
- downloads and validates the header chain and the blocks (initial block download);
- follows new blocks after the initial download (standard mode);
- switches to a competing chain when required (reorgs).

Not in v0.1: relaying blocks, relaying or accepting transactions, a mempool, mining, wallet.

Open questions are listed in [open-questions.md](open-questions.md) and referenced in the text as **[Qn]**.

## Modules

| Module                                     | Role                                                                                                                                       |
|--------------------------------------------|--------------------------------------------------------------------------------------------------------------------------------------------|
| [net](modules/net/README.md)               | Peer connections, ping/pong, routing of peer messages to the module that owns them, peers CRUD.                                            |
| [chain](modules/chain/README.md)           | Validation of headers and blocks, CRUD for tip, headers and blocks, tip updates, the UTXO set.                                             |
| [block-sync](modules/block-sync/README.md) | Initial sync, standard mode and reorgs: decides what to download, from which peer, which chain is active, and when the tip is rolled back. |

## Reading order

1. [architecture/overview.md](architecture/overview.md): modules, calls, queues, module boundaries, lifecycle.
2. [architecture/storage.md](architecture/storage.md): stores, atomicity, write ordering.
3. [modules/](modules/): one folder per module with entities, state and interface.
4. [flows/](flows/README.md): communication, processing headers, IBD, standard mode, reorg.
5. [open-questions.md](open-questions.md).

## Conventions

- **MUST / MUST NOT / SHOULD / MAY** are used in the RFC 2119 sense.
- Bitcoin Core functions and constants are named where behaviour must match them. BIPs are cited by number.

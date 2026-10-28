# Flows

| Flow | Covers |
|---|---|
| [communication.md](communication.md) | Connecting to a peer, pings, requesting a peer, processing a request |
| [initial-sync.md](initial-sync.md) | IBD: downloading headers, downloading blocks |
| [standard-mode.md](standard-mode.md) | New header, new transaction |
| [reorg.md](reorg.md) | Next header, next header (+t), current header or older |

In diagrams, `->>` with `-->>` is a direct call and its result, `-)` is a push to a queue or a P2P message.

# Flows

| Flow                                 | Covers                                                               |
|--------------------------------------|----------------------------------------------------------------------|
| [communication.md](communication.md) | Connecting to a peer, pings, requesting a peer, processing a request |
| [mode-ibd.md](mode-ibd.md)           | IBD: downloading headers, downloading blocks                         |
| [mode-standard.md](mode-standard.md) | Processing headers, block locator, appending a new header            |
| [mode-reorg.md](mode-reorg.md)       | Next header, next header (+t), current header or older               |

In diagrams, `->>` with `-->>` is a direct call and its result, `-)` is a push to a queue or a P2P message.

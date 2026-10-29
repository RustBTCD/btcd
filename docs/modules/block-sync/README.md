# block-sync

`block-sync` is the main initiator and processing center of the node:

- requests new headers and blocks;
- receives new headers and blocks;
- works with [chain](../chain/README.md) as the CRUD for headers and blocks;
- decides which blocks to download;
- decides which chain is active;
- decides whether the tip must be rolled back.

It selects peers using [net](../net/README.md) as the CRUD for peers.

### Modes

| Mode     | Flow                                             |
|----------|--------------------------------------------------|
| IBD      | [mode-ibd.md](../../flows/mode-ibd.md)           |
| Standard | [mode-standard.md](../../flows/mode-standard.md) |
| Reorg    | [mode-reorg.md](../../flows/mode-reorg.md)       |

During IBD and reorg, all other processes are paused, except maintaining the connections with peers. Messages in the
queue that are not part of the running IBD or reorg are skipped.

TBD (how we launch, how we switch from IBD to Standard)

### State

It only uses in-memory storage for non-persistent peers: mode, candidate headers, requests in flight, new block
batches (until included into chain), etc.

### Communication

The module accepts input messages from queue filled by `net`. It also may have triggers for request timeouts. The module
may call directly both `net` and `chain` modules:

| Module  | Calls                                                                                                |
|---------|------------------------------------------------------------------------------------------------------|
| `net`   | `get_peers`, `get_peer`, `send`, `disconnect`                                                        |
| `chain` | `validate_headers`, `store_headers`, `header`, `header_at`, `tip`, `connect_block`, `disconnect_tip` |

To obtain information from peers about the blockchain state, the module sends the following messages:

| Command      | Content                                              | Used in                                                            |
|--------------|------------------------------------------------------|--------------------------------------------------------------------|
| `getheaders` | protocol version, block locator, stop hash           | header download, last common ancestor search, inventory processing |
| `getdata`    | inventory items of type `MSG_WITNESS_BLOCK` (BIP144) | block download                                                     |

--- 

## Interface

`block-sync` exposes no calls to other modules. Its only input from other modules is its queue.

---

## Configuration

TBD

```toml
[block-sync]

```

---

## Special mechanics

### Peer selection

| Request                     | Peers eligible                                                                                                                  |
|-----------------------------|---------------------------------------------------------------------------------------------------------------------------------|
| Header download (IBD)       | one random connected peer                                                                                                       |
| Block download              | random connected peers whose features include `NODE_NETWORK` and `NODE_WITNESS`, and whose tip is at least the requested height |
| Last common ancestor search | the peer that sent the header                                                                                                   |

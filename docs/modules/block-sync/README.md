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
| IBD      | [initial-sync.md](../../flows/initial-sync.md)   |
| Standard | [standard-mode.md](../../flows/standard-mode.md) |
| Reorg    | [reorg.md](../../flows/reorg.md)                 |

During IBD and reorg, all other processes are paused, except maintaining the connections with peers.

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

| Command      | Content                                              | Used in                                      |
|--------------|------------------------------------------------------|----------------------------------------------|
| `getheaders` | protocol version, block locator, stop hash           | header download, last common ancestor search |
| `getdata`    | inventory items of type `MSG_WITNESS_BLOCK` (BIP144) | block download                               |

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
| Last common ancestor search | random connected peers                                                                                                          |

### Block locator

`getheaders` asks a peer for the headers of its chain that follow some block. We don't know in advance where the
peer's chain and ours diverge, so the request does not name one starting block. Instead it carries a **block
locator**: a list of block hashes from our chain, newest first. The peer matches the list against its own chain to
find the last block the two chains have in common, and answers from there.

How the peer answers (Bitcoin Core's `getheaders` handling):

1. It goes through the locator in order and takes the first hash that is on its own active chain. That block is on
   both chains.
2. It answers with the headers of its active chain that follow that block: at most 2000, ending with the stop hash if
   it reaches it. A zero stop hash means no limit other than 2000.
3. If no locator hash is on its active chain, it answers from the block after genesis.

The parent of the first header in the answer is the block found in step 1. So one answer shows where the peer's
chain leaves ours, and gives the headers that follow.

#### Full locator

A full locator finds that point in one round trip, however deep the divergence is. Like Core's `GetLocator`, it is
dense near the tip and sparse further back: the tip, the 10 blocks before it, then steps that double at every entry,
always ending with genesis. For a tip at height 1000 the heights are:

    1000, 999, …, 990, 988, 984, 976, 960, 928, 864, 736, 480, 0

About 20 hashes cover any chain length. The protocol allows at most 101.

In the `block-sync` we use the following locators:

| Request                                    | Locator                                                        | Stop hash                 | The answer gives                                                                                            |
|--------------------------------------------|----------------------------------------------------------------|---------------------------|-------------------------------------------------------------------------------------------------------------|
| First `getheaders` of a header download    | full locator of the active chain, built with `chain.header_at` | zero                      | up to 2000 headers of the peer's chain, starting after the last block it has in common with ours            |
| Next batch of a header download            | the last header received                                       | zero                      | the next up to 2000 headers of the peer's chain                                                             |
| Probe at height `m` of the ancestor search | only the active block at `m`                                   | the active block at `m+1` | whether `m` and `m+1` are on the peer's chain ([last common ancestor search](#last-common-ancestor-search)) |

An answer with fewer than 2000 headers means the peer has no more headers after it.

## Last common ancestor search

Binary search over the heights from 0 to the tip block's height on the active chain for the highest height whose block
is also on the peer's chain. A probe at height `m` is a `getheaders` with the locator and stop hash
given [above](#block-locator). The peer's `headers` answer means:

| Answer                                                                                                  | Meaning                                                |
|---------------------------------------------------------------------------------------------------------|--------------------------------------------------------|
| first header is the active block at `m+1`                                                               | `m` and `m+1` are common: continue above `m`           |
| first header's parent is the active block at `m`, and the first header is not the active block at `m+1` | the last common ancestor is `m`                        |
| empty                                                                                                   | `m` is the peer's tip: the last common ancestor is `m` |
| first header's parent is genesis, and `m > 0`                                                           | `m` is not on the peer's chain: continue below `m`     |

The search takes about log2 of the tip block's height probes.

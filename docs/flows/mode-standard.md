# Mode: Standard

In standard mode the node receives block announcements from connected peers. Because `net` sends `sendheaders` in the
connection flow, a peer normally announces a new block with a `headers` message carrying the block's header. A peer
falls back to an `inv` carrying only the block hash when it has more than 8 blocks to announce or does not yet know
which blocks the node has ([processing inventory](#processing-inventory)). For every announced header `block-sync`
decides whether it follows the tip or whether the chain has to be reorganized.

## Processing inventory

An `inv` message announces objects by hash only: for a block, `block-sync` learns the 32-byte block hash H. So,
`block-sync` uses `inv` only to learn about new headers. Upon receiving a message from the queue, it keeps
the entries of type `MSG_BLOCK` while skipping all other entries. For each kept block hash, `block-sync` calls
`chain.header(H)`, and skips hashes whose header is stored.

For a new hash, `block-sync` obtains the header by sending `getheaders` to the announcing peer with a full locator of
the active chain and `hash_stop = H`. This is the last common ancestor search of
[processing headers](#processing-headers), entered directly, since there is no parent to look up. The `headers` answer
carries the header of H and of every block between the active chain and H; from there the flow continues as for a
header received in a `headers` message.

Note, that `block-sync` does not answer an `inv` with `getdata` because blocks are requested only for stored headers.

```mermaid
sequenceDiagram
    autonumber
    participant P as peer
    participant N as net
    participant BS as block-sync
    participant C as chain
    P -) N: inv [MSG_BLOCK H, other entries]
    N -) BS: queue: {peer, "inv", inv}
    Note over BS: keep MSG_BLOCK entries<br/>skip the others
    BS ->> C: chain.header(H)
    C -->> BS: not found
    BS ->> N: net.send(peer, "getheaders", getheaders{locator, stop = H})
    N -) P: getheaders
    P -) N: headers (LCA + 1 ... H)
    N -) BS: queue: {peer, "headers", headers}
    Note over BS: processing headers
```

## Processing headers

### Problem statement

A header carries no height. Its height is the parent's height plus one, so it is known only when the parent is stored in
`chain`. For the headers with unknown parent we have to search the last common ancestor to understand the header
height and if the reorg is needed.

### Block locator

Before defining the flow, let's first examine what a block locator is. The `getheaders` is a main request we leverage to
fetch new blocks from chain. But its structure is more tricky than its name assumes:

| Field                | Content                                                                                            |
|----------------------|----------------------------------------------------------------------------------------------------|
| version              | Our protocol version. Core sends it but ignores it on receipt.                                     |
| hash count           | Number of locator hashes. At most 101 (`MAX_LOCATOR_SZ`); Core disconnects a peer that sends more. |
| block locator hashes | Hashes from our chain, newest first (the block locator).                                           |
| hash_stop            | Last header we want. All zeros means "as many as allowed".                                         |

The `getheaders` message asks a peer for the headers of its chain that follow some block. The node does
not know where the peer's chain and ours diverge, so the request can not name one starting block. Instead, the message
carries a **block locator**: a list of block hashes from our active chain, newest first. The peer matches the list
against its own chain to find a block both chains have, and answers from there.

The other peer (assuming it is Bitcoin Core) handles this request as follows:

1. It goes through the locator in order and takes the first hash that is on its own active chain. That block is on both
   chains.
2. It answers with the headers of its active chain that follow that block: at most 2000, ending with the stop hash if it
   reaches it. A zero stop hash means no limit other than 2000.
3. If no locator hash is on its active chain, it answers from the block after genesis.

The parent of the first header in the answer is the block found in step 1, so node then knows that block's height and
can count the heights of all headers in the answer.

A **full locator** finds a common block in one round trip, however far back the chains diverge. As in Core's
`GetLocator`, it is dense near the tip and sparse further back: the tip, the 10 blocks before it, then steps that double
at every entry, always ending with genesis. For a tip at height 1000 the heights are:

```
1000, 999, ..., 990, 988, 984, 976, 960, 928, 864, 736, 480, 0
```

About 20 hashes cover any chain length; the protocol allows at most 101. Because the locator is sparse further back, the
block found by the peer can be below the actual fork point. The headers between them are already on our active chain and
are skipped (step 3 below).

### Flow

So, imagine we received a header from a peer. After validation (if successful) we have to find its theoretical height.
We first try searching its parent in the `headers` table. If the entry exists, we will obtain all information we may
need (i.e. height). Otherwise, we start searching the last common ancestor (LCA). A block hash from an `inv` has no
known parent and starts the LCA search directly ([processing inventory](#processing-inventory)).

```mermaid
sequenceDiagram
    autonumber
    participant P as peer
    participant N as net
    participant BS as block-sync
    participant C as chain
    P -) N: header H
    N -) BS: queue: {peer, H}
    BS ->> C: chain.header(H.parent)
    C -->> BS: not found
    loop find all headers LCA + 1 ... H
        Note over BS: build locator
        BS ->> N: net.send(peer, "getheaders", getheaders{locator, stop = H})
        N -) P: getheaders
        P -) N: headers (LCA + 1 ... H)
        N -) BS: queue: {peer, headers}
    end
    BS ->> C: chain.validate_headers(LCA + 1 ... H)
    C -->> BS: ok
    Note over BS: H.height = LCA.height + number of headers<br/> H.chain_work = last_chain_work
```

If the LCA is the current tip we continue in the [standard flow](#append-new-header). Otherwise, the chain work of H
(`last_chain_work` returned by `validate_headers`) is compared with the chain work of the tip
([chain work](../modules/chain/README.md#chain-work)): if it is greater, we fall into [reorg mode](./mode-reorg.md);
if it is at most the chain work of the tip, the headers LCA + 1 ... H are not stored.

### Append new header

If the LCA is the current tip, we can just update it sequentially. The blocks `tip + 1 ... H` are requested from the
peer that announced H; if it does not answer, from random eligible
peers ([peer selection](../modules/block-sync/README.md#peer-selection)).

```mermaid
sequenceDiagram
    autonumber
    participant P1 as announcing peer, then random peers
    participant N as net
    participant BS as block-sync
    participant C as chain
    Note over BS: Has validated headers[tip + 1 ... H]
    BS ->> C: chain.store_headers(headers[tip + 1 ... H])
    C -->> BS: ok
    BS ->> N: net.send(announcing peer, "getdata", getdata [tip + 1 ... H])
    N -) P1: getdata
    P1 -) N: block [tip + 1 ... H]
    N -) BS: queue: {announcing peer, "block", block [tip + 1 ... H]}
    Note over BS: if no answer within the request timeout, or notfound —<br/>request the missing blocks from a random connected peer,<br/>until collects all of them
    loop for B in block [tip + 1 ... H]
        BS ->> C: chain.connect_block(B)
        alt Block valid and follows the tip
            Note over C: updates tip
            C -->> BS: ok
        else rejected
            C -->> BS: rejected {reason}
            Note over BS: break, request block again
        end
    end
```

A rejected block is handled as in [IBD](mode-ibd.md#downloading-blocks).



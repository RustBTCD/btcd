# Mode: Reorg

During a reorg all other processes are paused, except maintaining the connections with peers.

## Next header

Reorg happens if in the standard mode, we fund a valid chain of headers A..B, where A belongs to the node's current
chain, every header (A+1)..B does not belong to the node's chain and B > tip. It means that we found a chain that is
longer than ours — so we have to reorganize our node. The chain of A..B usually appears when the node receives a header
with unknown parent, so it starts searching LCA as described
in [processing-headers](mode-standard.md#processing-headers).

```mermaid
sequenceDiagram
    autonumber
    participant N as net
    participant BS as block-sync
    participant C as chain
    participant P as random peers
    Note over BS: knows alternative chain of headers tip-k ... tip+1, where tip-k is LCA
    BS ->> C: chain.validate_headers(tip-k+1 … tip+1)
    C -->> BS: ok
    BS ->> C: chain.store_headers(tip-k+1 … tip+1)
    Note over BS: reorg mode
    loop k times
        BS ->> C: chain.disconnect_tip()
        C -->> BS: new tip
    end
    Note over BS: tip block at height tip-k
    loop block \in tip-k+1 ... tip+1
        BS ->> N: net.send(peer, "getdata", getdata block)
        P -) N: block
        N -) BS: queue: {peer, block}
        BS ->> C: chain.connect_block(block)
        C -->> BS: new tip
    end
    Note over BS: standard mode
```

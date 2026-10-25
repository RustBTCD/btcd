# Initial sync (IBD)

Headers first, then blocks in parallel from many peers, connected in order. Rules:
[block-sync](../daemons/block-sync/README.md), [chain](../daemons/chain/README.md), [utxo](../daemons/utxo/README.md).

## Header sync

```mermaid
sequenceDiagram
    autonumber
    participant P as sync peer P1 (outbound)
    participant N as net
    participant BS as block-sync
    participant C as chain

    N-)BS: net.peer_connected (P1, outbound, NODE_NETWORK)
    Note over BS: ibd = true and no sync peer:<br/>P1 becomes the sync peer
    BS->>C: chain.locator
    C-->>BS: [genesis]
    BS-)N: net.send getheaders
    N->>P: getheaders
    P->>N: headers (2000)
    N-)BS: net.headers
    BS->>C: chain.submit_headers
    C-->>BS: ok, last = h2000, best_header = h2000
    BS->>C: chain.locator (from h2000)
    C-->>BS: locator
    BS-)N: net.send getheaders
    Note over BS,C: repeats until a headers message has fewer than 2000
    Note over BS: after every submit_headers result:<br/>schedule block download (below)
```

- Block download starts as soon as the first batch of headers is accepted; it does not wait for header sync to finish.
- During IBD, headers and block `inv`s from inbound peers are ignored, and only one outbound peer serves headers.
- No progress for `headers_timeout_secs` → `net.disconnect` the sync peer, pick another outbound peer.

## Block download

```mermaid
sequenceDiagram
    autonumber
    participant P1 as peer P1
    participant P2 as peer P2
    participant N as net
    participant BS as block-sync
    participant C as chain
    participant U as utxo

    BS->>C: chain.missing_blocks (window 1024)
    C-->>BS: [h1 … h1024]
    Note over BS: up to 16 in flight per peer
    BS-)N: net.send getdata (h1–h16) to P1
    BS-)N: net.send getdata (h17–h32) to P2
    N->>P1: getdata
    N->>P2: getdata
    P2->>N: block h17
    N-)BS: net.block (P2, h17)
    BS->>C: chain.submit_block (requested)
    Note over C: checks pass, store durably<br/>h1–h16 missing, nothing to connect
    C-->>BS: stored
    P1->>N: block h1
    N-)BS: net.block (P1, h1)
    BS->>C: chain.submit_block (requested)
    Note over C: store, activate
    C->>U: utxo.connect_block h1
    U-->>C: ok
    C-)BS: chain.tip_changed (tip h1)
    C-->>BS: connected
    Note over BS: refill: chain.missing_blocks,<br/>assign freed slots
```

- Blocks arrive in any order. `chain` stores each one after its own checks and connects only along the chain, so `h17`
  waits until `h2`–`h16` are stored.
- When the gap fills, one `chain.submit_block` call connects a whole run of stored blocks, one `utxo.connect_block` per
  block, and `chain.tip_changed` is split into events of at most 128 blocks.
- During IBD, `block-sync` does not announce blocks.
- Script checks are skipped for blocks below the assumevalid block ([chain](../daemons/chain/README.md#assumevalid));
  every other check runs.

## Stalling

```mermaid
sequenceDiagram
    participant N as net
    participant BS as block-sync
    participant C as chain
    Note over BS: window exhausted: h1 … h1024 are stored or in flight,<br/>tip is h0, h1 in flight at P1 for longer than stall timeout
    BS->>N: net.disconnect (P1, stalling)
    N-->>BS: {}
    N-)BS: net.peer_disconnected (P1)
    Note over BS: h1 … h16 missing again<br/>stall timeout doubles (2 s → 4 s)
    BS->>C: chain.missing_blocks
    C-->>BS: [h1 … h16]
    BS-)N: net.send getdata (h1–h16) to P3
```

A block requested more than `block_timeout_secs` ago is handled the same way, regardless of the window.

## End of IBD

`chain` computes `ibd = false` once the tip has at least `min_chain_work` and is younger than `max_tip_age_hours`.
The next `chain.tip_changed` carries `ibd: false`:

- `block-sync` starts announcing new blocks, serves `getheaders`, and sends `getheaders` to every peer.

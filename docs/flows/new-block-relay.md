# New block relay

Steady state after IBD: a peer announces a new block; the node fetches it, connects it, and announces it to its other
peers. Rules: [block-sync](../daemons/block-sync/README.md#announcing-new-blocks).

## Announcement by headers (BIP130)

```mermaid
sequenceDiagram
    autonumber
    participant P1 as peer P1 (sender)
    participant P2 as peer P2 (prefers headers)
    participant P3 as peer P3 (inv)
    participant N as net
    participant BS as block-sync
    participant C as chain
    participant U as utxo

    P1->>N: headers [H]
    N-)BS: net.headers (P1, [H])
    BS->>C: chain.submit_headers
    C-->>BS: ok, best_header = H (no data)
    Note over BS: P1 best known := H
    BS->>C: chain.missing_blocks
    C-->>BS: [H]
    BS-)N: net.send getdata witness_block H to P1
    N->>P1: getdata
    P1->>N: block H
    N-)BS: net.block (P1, H)
    BS->>C: chain.submit_block (H, requested)
    C->>U: utxo.connect_block H
    U-->>C: ok
    C-)BS: chain.tip_changed (tip H, connected [H + header], ibd false)
    C-->>BS: connected
    Note over BS: announce to peers whose best known is not H
    BS-)N: net.send headers [H] to P2
    BS-)N: net.send inv H to P3
    N->>P2: headers
    N->>P3: inv
```

- P1 is skipped in the announcement because its best known block is already H.
- If more than 8 blocks were connected at once (for example after a reorg), peers get an `inv` for the tip instead of
  `headers`.

## Announcement by `inv`

```mermaid
sequenceDiagram
    participant P as peer P3
    participant N as net
    participant BS as block-sync
    participant C as chain
    P->>N: inv block H
    N-)BS: net.inv_blocks (P3, [H])
    Note over BS: H unknown to chain
    BS->>C: chain.locator
    C-->>BS: locator
    BS-)N: net.send getheaders to P3
    P->>N: headers [H]
    Note over BS: continues as in the headers flow
```

## Serving the block to a peer

```mermaid
sequenceDiagram
    participant P as peer P3
    participant N as net
    participant BS as block-sync
    participant C as chain
    P->>N: getdata witness_block H
    N-)BS: net.getdata_blocks (P3, [witness_block H])
    BS->>C: chain.block (H, verbosity 0)
    C-->>BS: hex, in_active_chain true
    BS-)N: net.send block (witness true) to P3
    N->>P: block H
```

Serving a peer's `getheaders` follows the same pattern with `chain.headers_after` and a `headers` message, and is
skipped during IBD.

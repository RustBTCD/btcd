# Transaction relay

Rules: [tx-relay](../daemons/tx-relay/README.md), [utxo.check_tx](../daemons/utxo/rpc.md#utxocheck_tx), [hook table](../architecture/events-and-hooks.md#hook-table).

## A peer announces a transaction

```mermaid
sequenceDiagram
    autonumber
    participant P1 as peer P1 (wtxid relay)
    participant P2 as peer P2 (relay, feefilter ok)
    participant N as net
    participant TR as tx-relay
    participant U as utxo
    participant H as hook subscribers (none in v0.1)

    P1->>N: inv wtx W
    N-)TR: net.inv_txs (P1, [wtx W])
    Note over TR: not cached, not rejected,<br/>not requested yet
    TR-)N: net.send getdata wtx W to P1
    N->>P1: getdata
    P1->>N: tx T
    N-)TR: net.tx (P1, T)
    TR->>U: utxo.check_tx (T)
    U-->>TR: ok: fee, vsize, weight, sigop cost, inputs
    Note over TR: policy: weight, sigops, fee rate<br/>conflicts: none
    TR->>H: hook.tx_accept
    H-->>TR: continue
    Note over TR: admit to relay cache,<br/>emit tx-relay.tx_accepted,<br/>queue for P2
    Note over TR: trickle timer for P2 fires
    TR-)N: net.send inv wtx W to P2
    N->>P2: inv
    P2->>N: getdata wtx W
    N-)TR: net.getdata_txs (P2, [wtx W])
    Note over TR: cached and announced to P2
    TR-)N: net.send tx T to P2
    N->>P2: tx
```

Rejections:

| `utxo.check_tx` or check result | tx-relay does |
|---|---|
| `missing_inputs` | remembers T as rejected until the next tip change; nothing else |
| `invalid` | remembers T as rejected; `net.misbehaving` for P1 |
| policy failure, conflict, hook reject, cache full | remembers T as rejected |

A peer without wtxid relay announces and requests by txid (`tx`, `witness_tx`) instead of wtxid; the flow is the same.

## The operator submits a transaction

```mermaid
sequenceDiagram
    actor O as operator
    participant M as rbtcd
    participant TR as tx-relay
    participant U as utxo
    participant N as net
    O->>M: rbtc tx-relay submit 0200…
    M->>TR: tx-relay.submit (from operator)
    TR->>U: utxo.check_tx
    U-->>TR: ok
    Note over TR: policy, conflicts, hook.tx_accept,<br/>admit, queue for all relaying peers
    TR-->>M: txid, wtxid, fee, vsize
    M-->>O: result
    Note over TR: announced on the next trickle of each peer
    TR-)N: net.send inv …
```

Errors go back to the operator (`invalid`, `missing_inputs`, `rejected`); no peer is punished. During IBD the submission is refused with `rejected` / `initial-block-download`.

## A block confirms or conflicts with cached transactions

```mermaid
sequenceDiagram
    participant C as chain
    participant TR as tx-relay
    participant U as utxo
    C-)TR: chain.tip_changed (connected [B])
    TR->>U: utxo.unspent (all inputs of cached transactions)
    U-->>TR: unspent flags at tip B
    Note over TR: remove every transaction with a spent input<br/>(confirmed in B, or conflicting with B)<br/>emit tx-relay.tx_removed per transaction<br/>clear recent rejects
```

If the cache is empty, no call is made.

## Expiry and eviction

- Every minute, entries older than `expiry_hours` are removed (`tx-relay.tx_removed`, reason `expired`).
- When a new transaction does not fit in `max_cache_mib`, entries with a lower fee rate are evicted first (`evicted`). If that is not enough, the new transaction is rejected.

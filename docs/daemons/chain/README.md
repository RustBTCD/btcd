# chain

Owns everything about headers and blocks, chooses the best chain, and drives `utxo` to follow it.

## Responsibilities

- **Block index.** Every header known, with height, cumulative work, status and block location.
  Kept fully in memory, persisted in the `index` store ([storage.md](../../architecture/storage.md#chain)).
- **Block storage.** Block bodies in the `blocks` flat-file store.
- **Header acceptance.** Core's `CheckBlockHeader` (proof of work against the target) and `ContextualCheckBlockHeader`
  (`GetNextWorkRequired`, timestamp above the median time past and at most 2 hours in the future, minimum block versions
  after BIP34, BIP66 and BIP65). A header whose ancestor is invalid is invalid.
- **Block acceptance.** Core's `CheckBlock` (merkle root, size limits, exactly one coinbase at position 0,
  `CheckTransaction` for each transaction, legacy sigop limit) and `ContextualCheckBlock` (transaction finality against
  the median time past, BIP34 coinbase height, BIP141 witness commitment, weight limit). Accepted blocks are stored
  durably.
- **Best-chain activation.** Core's `ActivateBestChain`: pick the most-work chain whose blocks are all stored and not
  known invalid, disconnect the active chain down to the fork point, connect the new blocks one by one through `utxo`,
  and fall back to the next candidate when a block fails.
- **Rules per block.** Decide everything that depends on the height or on the chain and pass it to `utxo.connect_block`:
  the script flags (Core's `GetBlockScriptFlags`, including its exception blocks), whether script checks may be skipped
  (assumevalid), whether BIP30 applies (Core's BIP30 conditions and its two exception blocks), and whether BIP68
  sequence locks apply (CSV active).
- **IBD state.** Core's `IsInitialBlockDownload`: true while the tip's chain work is below `min_chain_work` or the tip
  is older than `max_tip_age_hours`. Once false it stays false until restart.
- **Queries.** Chain state, headers, blocks, chain tips, transactions in a given block.
- **Helpers for block-sync.** Locators, headers to serve, blocks to download next.

## Not responsibilities

- Coins, scripts, fees, sequence locks (`utxo`).
- Talking to peers or deciding which peer to ask (`block-sync`).
- Persisting the active tip ([storage.md](../../architecture/storage.md#the-tip-rule)).

## Block submission

`chain.submit_block` processes a block in this order:

1. Decode it. If the block is already stored: `duplicate`.
2. If its header is unknown, accept the header as in `chain.submit_headers`. An unknown parent gives `unconnected`.
3. If the header or an ancestor is marked invalid: error `invalid`.
4. If the block was not requested (`requested: false`) and does not have more work than the tip, or is more than 288
   blocks ahead of it: `ignored`, as Core's `AcceptBlock` does for unrequested blocks.
5. Run `CheckBlock` and `ContextualCheckBlock`. On failure:
   - if the failure means the block data was **mutated** (bad merkle root, duplicate transactions, bad witness nonce or
     commitment, unexpected witness), the header is **not** marked invalid. Error `invalid` with `data.mutated = true`;
   - otherwise mark the block invalid and its descendants as having an invalid parent. Error `invalid`.
6. Store the block body and its index entry ([write ordering](../../architecture/storage.md#write-ordering)).
7. Run best-chain activation. If this block, or an ancestor connected in the same activation, fails in
   `utxo.connect_block`, the error is returned: the block is then invalid or has an invalid parent.
   Failures of unrelated blocks during the same activation are logged, not returned.
8. Return `connected` if the block is now on the active chain, otherwise `stored`.

## Best-chain activation

```
loop:
    candidate = most-work header whose chain is entirely stored and not invalid
    if candidate.work <= tip.work: stop
    fork = last common ancestor of tip and candidate
    while tip != fork:
        utxo.disconnect_block(tip, body)        # always succeeds for a valid tip
        tip = tip.prev; emit chain.block_disconnected
    for block in fork+1 ..= candidate:
        utxo.connect_block(block, height, rules_for(block))   # script_flags, verify_scripts, enforce_bip30, enforce_bip68
        on invalid: mark block invalid and descendants failed_parent; continue loop
        tip = block; emit chain.block_connected
    emit chain.tip_changed
```

- When a branch fails half-way, the loop picks the best remaining candidate. That may be the old branch, which is then
  connected back.
- `chain.tip_changed` is emitted when an activation ends with a different tip than it started with.
  It describes the net change: blocks of the old chain above the fork point (`disconnected`) and blocks of the new chain
  above it (`connected`). An event never lists more than 128 connected blocks; long runs (for example during IBD, when a
  missing block fills a gap) are split into several events. `chain.block_connected` and `chain.block_disconnected` are
  emitted for every individual step, including steps that a failed branch later undoes.
- Activation runs inside the RPC that triggered it, so the caller's response comes after the tip has moved.

## Assumevalid

Script checks are skipped for a block (`verify_scripts: false` in `utxo.connect_block`) when all of these hold, as in
Core's `ConnectBlock`:

- the configured assumevalid block is in the block index and on the best header chain;
- the block being connected is an ancestor of it;
- the best header has at least `min_chain_work`;
- the best header is at least two weeks' worth of proof of work ahead of the block.

Every other check still runs. `assume_valid = "none"` verifies everything.

## State

| State | Persistent |
|---|---|
| Block index: header, height, work, status, location per block | yes, store `index` |
| Block bodies | yes, store `blocks` |
| Best header | derived at startup |
| Active chain (height → block) | in memory, rebuilt from `utxo`'s tip ([storage.md](../../architecture/storage.md#startup-reconciliation)) |
| IBD flag | in memory |

## Manifest

| | |
|---|---|
| source | no |
| subscribes | — |
| emits events | [events.md](events.md) |
| emits hooks | — |
| serves RPCs | [rpc.md](rpc.md) |
| calls | `utxo.info`, `utxo.connect_block`, `utxo.disconnect_block` |

## Concurrency

Writes (`submit_headers`, `submit_block`) are processed one at a time in arrival order. Read RPCs run concurrently
against the in-memory index and the database and never wait for an activation to finish. They may observe the tip before
or after a concurrent change.

## Configuration

```toml
[chain]
assume_valid = "default"     # "default" (Core's value for this network), "none", or a block hash
min_chain_work = "default"   # "default" (Core's value for this network) or Work hex
max_tip_age_hours = 24
```

Consensus parameters (proof-of-work limit, retarget rules, deployment heights) come from rust-bitcoin's network
parameters and are not configurable.

## Not in v0.1

- Headers presync (Core's `HeadersSyncState`). Headers are accepted from the peers `block-sync` selects
  ([block-sync](../block-sync/README.md#header-synchronisation)).
- `invalidateblock`, `reconsiderblock`.
- Pruning.

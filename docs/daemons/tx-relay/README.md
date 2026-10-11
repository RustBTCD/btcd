# tx-relay

Receives, validates and relays unconfirmed transactions, and accepts transactions from the operator. In v0.1 it keeps them in a bounded in-memory **relay cache**.

## Responsibilities

- Per-peer transaction relay state: whether the peer wants transactions, wtxid relay (BIP339), its fee filter (BIP133), what it already knows.
- Requesting announced transactions, with per-peer limits and timeouts.
- Validating received transactions: consensus through `utxo.check_tx`, then tx-relay's own policy and conflict checks, then `hook.tx_accept`.
- The relay cache: admission, conflict tracking, size limit and eviction, expiry, removal after blocks.
- Announcing accepted transactions to peers, and serving them on request.
- Operator submission (`tx-relay.submit`).

## Not responsibilities

- Consensus validation of transactions (`utxo`).
- Unconfirmed chains, orphans, replacement (RBF), packages: not in v0.1.

## Relay cache

Each entry: transaction, txid, wtxid, fee, vsize, fee rate, spent outpoints, time received, source peer (or none for the operator).

| Rule | v0.1 |
|---|---|
| Inputs | Must all be in the UTXO set. A transaction spending another unconfirmed transaction is rejected (`missing_inputs`). |
| Conflicts | First seen wins. A map from spent outpoint to cached transaction rejects later spenders with `txn-mempool-conflict`, Core's reason. |
| Policy | Weight at most 400 000 (Core's `MAX_STANDARD_TX_WEIGHT`), sigop cost at most 16 000 (Core's `MAX_STANDARD_TX_SIGOPS_COST`), fee rate at least `min_relay_feerate`. |
| Size limit | `max_cache_mib`. When a new transaction does not fit, entries with a lower fee rate than the new one are evicted, lowest first. If that does not free enough space, the new transaction is rejected (`mempool full`). |
| Expiry | Entries older than `expiry_hours` are removed. |
| After blocks | On every `chain.tip_changed` that connected or disconnected blocks, tx-relay asks `utxo.unspent` for all spent outpoints of the cache and removes every transaction with an input that is no longer unspent: it was confirmed or conflicts with a confirmed transaction. |
| Reorgs | Transactions from disconnected blocks are not re-added. Peers will announce them again. |

Recently rejected transactions are remembered by wtxid and txid (rolling set of 50 000) so they are not downloaded again. The set is cleared on every tip change, as Core clears its recent rejects.

## Processing a transaction

```
received tx (from a peer, or tx-relay.submit)
  ├─ from a peer and not requested from it?            → ignore
  ├─ already cached or recently rejected?              → ignore (submit: return the cached result)
  ├─ utxo.check_tx
  │    ├─ missing_inputs                               → remember as rejected; drop
  │    ├─ invalid                                      → remember as rejected; net.misbehaving(peer)
  │    └─ ok: fee, vsize, weight, sigop cost, inputs
  ├─ policy (weight, sigops, fee rate)                 → reject
  ├─ conflict with a cached transaction                → reject: txn-mempool-conflict
  ├─ hook.tx_accept                                    → reject: rejected by hook
  ├─ make room (eviction)                              → reject: mempool full
  └─ admit, emit tx-relay.tx_accepted, queue announcements
```

Rejected transactions from an operator submission return the error to the operator and never cause `net.misbehaving`.

## Peers

On `net.peer_connected`, a peer gets relay state. tx-relay sends it a `feefilter`: during IBD with the maximum value, which tells the peer not to announce anything (as Core does); otherwise with `min_relay_feerate`. When IBD ends, every peer gets a new `feefilter` with the real value.

### Downloading announced transactions

On `net.inv_txs`, ignored during IBD:

- From a peer with wtxid relay, only `wtx` items are used; from others, only `tx` items (as Core).
- For each hash that is not cached, not recently rejected, and not already requested: send `getdata` (`wtx` for wtxid-relay peers, otherwise `witness_tx`) to the announcing peer, if it has fewer than `max_requests_per_peer` outstanding requests. Other peers that announce the same hash are remembered as further announcers.
- On `net.notfound_txs`, on a timeout of `request_timeout_secs`, or when the peer disconnects, the request moves to the next remembered announcer.
- The announcing peer is marked as knowing the transaction.

### Announcing

Accepted transactions are queued for every peer that has `relay` set, is not the source, has a fee filter at or below the transaction's fee rate, and does not already know the transaction.

Each peer's queue is flushed as one `inv` at random intervals with exponential distribution, mean 2 s for outbound and 5 s for inbound peers (Core's trickle). At most 1000 items per `inv`, highest fee rate first. Items are `wtx` with the wtxid for wtxid-relay peers, `tx` with the txid otherwise.

### Serving

On `net.getdata_txs`: transactions that are cached **and were announced to that peer** are sent with `net.send` (`witness: false` for `tx` items). Everything else is answered with one `notfound`, as Core does.

## Manifest

| | |
|---|---|
| source | no |
| subscribes | `net.peer_connected`, `net.peer_disconnected`, `net.inv_txs`, `net.tx`, `net.getdata_txs`, `net.notfound_txs`, `net.feefilter`, `chain.tip_changed` |
| emits events | [events.md](events.md) |
| emits hooks | `tx_accept` |
| serves RPCs | [rpc.md](rpc.md) |
| calls | `utxo.check_tx`, `utxo.unspent`, `chain.info`, `net.send`, `net.misbehaving` |

`chain.info` is called once during initialisation to learn the IBD state before the first `chain.tip_changed`.

## Concurrency

One task owns the cache and per-peer state and processes events and RPCs in order. Validation calls to `utxo.check_tx` are made one at a time.

## Configuration

```toml
[tx-relay]
min_relay_feerate = 100        # sat/kvB; Bitcoin Core 30's -minrelaytxfee default
max_cache_mib = 64
expiry_hours = 336
max_requests_per_peer = 100
request_timeout_secs = 60
```

## Not in v0.1

- Unconfirmed chains (transactions spending unconfirmed outputs), orphans, packages.
- Replacement (RBF).
- Policy script flags, `IsStandardTx` and `AreInputsStandard` checks, dust limit.
- Re-adding transactions from disconnected blocks.

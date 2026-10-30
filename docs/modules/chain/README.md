# chain

`chain` accepts new blocks, validates them, appends them to the current active chain and updates the tip. It is the CRUD
for tip, headers and blocks. It has no business logic except validating headers and blocks and updating the tip when a
block is sequential (its parent is the tip block).

`chain` maintains the UTXO set of the active chain. The tip is stored together with the UTXO set, so every change of the
UTXO set and the tip is atomic.

Decisions about which headers and blocks to fetch, which chain is active, and when the tip is rolled back belong
to [block-sync](../block-sync/README.md).

--- 

## State

| State                                                            | Store, table           |
|------------------------------------------------------------------|------------------------|
| UTXO set                                                         | `chainstate`, `coins`  |
| Undo data per block                                              | `chainstate`, `undo`   |
| Active chain by height: block hash, block time, median time past | `chainstate`, `active` |
| Tip                                                              | `chainstate`, `meta`   |
| Valid headers with height, cumulative chain work                 | `headers`, `headers`   |
| Blocks                                                           | `blocks`, `blocks`     |

Stores and tables: [storage.md](../../architecture/storage.md).

---

## Genesis

On the very first start `chain` stores the network's genesis header and block, and sets the tip to the genesis block at
height 0. The genesis coinbase output is not added to the UTXO set, as in Bitcoin Core.

---

## Validation

Consensus rules are the ones active at the height of the header or block on the network in use (main, testnet4, signet,
regtest), with the deployment heights of rust-bitcoin's network parameters.

### Headers

As Bitcoin Core's `CheckBlockHeader` and `ContextualCheckBlockHeader`:

- proof of work: the header hash is at most the target encoded in `bits`;
- `bits` equals the required target (`GetNextWorkRequired`);
- timestamp above the median time past of the previous 11 blocks and at most 2 hours in the future;
- version at least the minimum required after BIP34, BIP66 and BIP65.

Contextual checks need the parent header, so the parent of the first header of a call MUST be stored.

### Chain work

The work of a header is the expected number of hash evaluations needed to meet its target, as Bitcoin Core's
`GetBlockProof`:

```
target = the 256-bit integer encoded by the header's `bits`
work   = floor(2^256 / (target + 1))
```

The chain work of a header is the sum of the work of the header and of all its ancestors: the chain work of the genesis
header is its work; the chain work of every other header is the chain work of its parent plus its work.
`validate_headers` computes it from `bits` after the `bits` check above and returns it as `last_chain_work`;
`store_headers` stores it in the header entry. The chain work of the tip is the chain work of the tip block's header
entry, returned by `tip`.

Two header chains are compared by chain work, never by height: the chain whose last header has the greater chain work
has more work. On equal chain work the node keeps its current chain, as Bitcoin Core.

### Blocks

As Bitcoin Core's `CheckBlock` and `ContextualCheckBlock`:

- merkle root, no duplicate transactions, size and weight limits, exactly one coinbase at position 0, `CheckTransaction`
  for every transaction, legacy sigop limit;
- transaction finality, BIP34 coinbase height, BIP141 witness commitment.

As Bitcoin Core's `ConnectBlock`, against the UTXO set:

- BIP30: a transaction does not overwrite an unspent output (with Core's exceptions);
- `Consensus::CheckTxInputs`: inputs exist and are unspent, coinbase maturity (100 blocks), amounts in range, inputs
  cover outputs;
- BIP68 sequence locks, using the median time past stored in `active`;
- block sigop cost limit (80 000), counting P2SH and witness sigops;
- script verification of every input;
- coinbase outputs at most subsidy plus fees.

Rejections carry Bitcoin Core's reject reason (`high-hash`, `bad-diffbits`, `bad-txnmrklroot`,
`bad-txns-inputs-missingorspent`, `bad-cb-amount`, etc.).

---

## Interface

Direct calls. In v0.1 the only caller is `block-sync`. Calls that write are executed one at a time.

### Headers

| Call               | Input                                               | Output                                                                                                                                          |
|--------------------|-----------------------------------------------------|-------------------------------------------------------------------------------------------------------------------------------------------------|
| `validate_headers` | headers in chain order, each extending the previous | `ok {first_height, last_height, last_chain_work}`, or `rejected {index, reason}`, or `not_connected` if the first header's parent is not stored |
| `store_headers`    | headers for which `validate_headers` returned `ok`  | stored with height and chain work; `not_connected` if the first header's parent is not stored                                                   |
| `header`           | block hash                                          | stored header entry `{header, height, chain_work, has_block}`, or not found; `has_block` is true when the block is in `blocks`                  |
| `header_at`        | height                                              | header entry of the active chain at that height, or not found                                                                                   |

`validate_headers` writes nothing.

### Blocks

| Call            | Input      | Output                          |
|-----------------|------------|---------------------------------|
| `connect_block` | block      | new tip, or `rejected {reason}` |
| `block`         | block hash | stored block, or not found      |

`connect_block`:

1. The block's header MUST be stored; otherwise `rejected {unknown-header}`.
2. The block's parent MUST be the tip block; otherwise `rejected {not-sequential}`.
3. Run the [block validation](#blocks). If invalid: `rejected {reason}` and nothing is written.
4. Store the block, apply it to the UTXO set and move the
   tip ([storage.md](../../architecture/storage.md#connecting-a-block-to-the-tip-block)).

### Tip and UTXO set

| Call             | Input    | Output                                                                                                                                                                                                                                            |
|------------------|----------|---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `tip`            | —        | `{hash, height, chain_work}` of the tip block                                                                                                                                                                                                     |
| `disconnect_tip` | —        | new tip. Removes the outputs the tip block created, restores the coins from its `undo` entry and moves the tip to its parent ([storage.md](../../architecture/storage.md#disconnecting-the-tip-block)). The genesis block cannot be disconnected. |
| `coin`           | outpoint | the unspent output, or not found                                                                                                                                                                                                                  |

---

## Configuration

```toml
[chain]
chainstate_cache_mib = 512 # RocksDB block cache of the chainstate store
headers_cache_mib = 128    # RocksDB block cache of the headers store
blocks_cache_mib = 128     # RocksDB block cache of the blocks store
```

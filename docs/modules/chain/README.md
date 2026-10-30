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
regtest). The `chain` module validates headers and blocks with the constants of the
[consensus parameters](#consensus-parameters) table below. From rust-bitcoin's network parameters `chain` takes only
the proof-of-work parameters: proof-of-work limit, target spacing, target timespan, the min-difficulty flag and the
no-retargeting flag.

### Consensus parameters

Values as in Bitcoin Core `kernel/chainparams.cpp`. A rule "from height h" is active for blocks at height h and above.

| Constant                               | main                                                                            | testnet4 | signet    | regtest |
|----------------------------------------|---------------------------------------------------------------------------------|----------|-----------|---------|
| BIP34 height (height in coinbase)      | 227931, hash `000000000000024b89b42a942fe0d9fea3bb44ab7bd1b19115dd6a759c0808b8` | 1        | 1         | 1       |
| BIP66 height (strict DER)              | 363725                                                                          | 1        | 1         | 1       |
| BIP65 height (CHECKLOCKTIMEVERIFY)     | 388381                                                                          | 1        | 1         | 1       |
| CSV height (BIP68, BIP112, BIP113)     | 419328                                                                          | 1        | 1         | 1       |
| SegWit height (BIP141, BIP143, BIP147) | 481824                                                                          | 1        | 1         | 0       |
| Subsidy halving interval, blocks       | 210000                                                                          | 210000   | 210000    | 150     |
| Coinbase maturity, blocks              | 100                                                                             | 100      | 100       | 100     |
| BIP94 enforced (testnet4 rules)        | no                                                                              | yes      | no        | no      |
| Signet block signature (BIP325)        | no                                                                              | no       | yes       | no      |
| Signet challenge                       | —                                                                               | —        | see below | —       |

Signet challenge (default signet), hex:

```
512103ad5e0edad18cb1f0fc0d28a3d4f1f3e445640337489abb10404f2d1e086be430210359ef5021964fe22d6f8e05b2463c9540ce96883fe3b278760f048f5189f2e6c452ae
```

Script verification flags of a block, as Bitcoin Core's `GetBlockScriptFlags`:

| Flag                         | Active                 |
|------------------------------|------------------------|
| `P2SH`, `WITNESS`, `TAPROOT` | every block            |
| `DERSIG`                     | from the BIP66 height  |
| `CHECKLOCKTIMEVERIFY`        | from the BIP65 height  |
| `CHECKSEQUENCEVERIFY`        | from the CSV height    |
| `NULLDUMMY`                  | from the SegWit height |

Two main blocks are exceptions and are verified with the listed flags only:

| Block hash                                                         | Flags             |
|--------------------------------------------------------------------|-------------------|
| `00000000000002dc756eebf4f49723ed8d30cc28a5f108eb94b1ba88ac4f9c22` | none              |
| `0000000000000000000f14c35b2d841e986ab5441de8c585d5ffe55ea1e395ad` | `P2SH`, `WITNESS` |

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

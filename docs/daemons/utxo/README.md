# utxo

Owns the UTXO set and everything that needs it: connecting and disconnecting blocks, and validating loose transactions. Its committed tip is the node's single record of the active chain ([storage.md](../../architecture/storage.md#the-tip-rule)).

The guiding split with `chain`: **`chain` decides which rules apply at a height; `utxo` applies them.** `utxo` has no knowledge of deployments, assumevalid or chain selection. It receives those decisions as parameters of `utxo.connect_block`.

## Responsibilities

- **The UTXO set** and per-block undo data, with set statistics (coin count, total amount).
- **The active chain by height** (hash, time, median time past), needed for BIP68 time locks and for `chain`'s startup reconciliation.
- **Connecting a block** (Core's `ConnectBlock`), atomically, on top of the current tip:
  - BIP30 (no transaction may overwrite an unspent output), when `chain` says it applies;
  - `Consensus::CheckTxInputs` per transaction: inputs exist and are unspent, coinbase maturity (100 blocks), amounts in range, inputs cover outputs;
  - BIP68 sequence locks (Core's `SequenceLocks`), when `chain` says they apply;
  - block sigop cost limit (80 000), counting P2SH and witness sigops;
  - script verification for every input with the flags `chain` provides, unless `verify_scripts` is false;
  - coinbase outputs at most subsidy plus fees.
  Either the whole block is applied in one write batch or nothing changes.
- **Disconnecting the tip block** (Core's `DisconnectBlock`) using its undo data, atomically.
- **Validating a loose transaction** against the current tip for tx-relay (`utxo.check_tx`).
- **Queries**: one coin, existence of many outpoints, set information.

Script verification uses libbitcoinkernel through the `bitcoinkernel` crate.

## Not responsibilities

- Header, block structure and context checks (`chain`).
- Choosing which block to connect, or reorganising (`chain` drives every connect and disconnect).
- Relay policy: fee rates, transaction weight limits (`tx-relay`).
- A mempool (not in v0.1).

## Genesis

The genesis block is connected like any other block at height 0 with no parent, but its coinbase output is not added to the set (it is unspendable, as in Core).

## Loose transaction validation

`utxo.check_tx` validates a transaction as if it were included in the next block (height = tip + 1, median time past of the tip):

1. `CheckTransaction`; the transaction must not be a coinbase.
2. Finality (`IsFinalTx`) for the next block.
3. Every input exists in the UTXO set; otherwise `missing_inputs` with the missing outpoints.
4. `Consensus::CheckTxInputs` and BIP68 sequence locks for the next block.
5. Scripts with every consensus flag active at the next block. Never skipped.

It reports fee, size and sigop cost for tx-relay's policy. It does not consult or change any relay state, so two transactions spending the same coin both pass; detecting that conflict is tx-relay's job in v0.1.

Only consensus script flags are applied. Core's policy-only flags (`DISCOURAGE_UPGRADABLE_NOPS`, `CLEANSTACK`, `MINIMALDATA`, …) are not.

## State

| State | Persistent |
|---|---|
| Coins, undo data, active chain by height, tip, statistics | yes, store `chainstate` ([storage.md](../../architecture/storage.md#utxo)) |

No in-memory coin cache in v0.1; each block is one write batch.

## Manifest

| | |
|---|---|
| source | no |
| subscribes | — |
| emits events | — |
| emits hooks | — |
| serves RPCs | [rpc.md](rpc.md) |
| calls | — |

`utxo` calls no other daemon. It is initialised first and shut down last.

## Concurrency

`connect_block` and `disconnect_block` run one at a time. Read RPCs (`info`, `coin`, `unspent`, `check_tx`) run concurrently with them, each against a database snapshot taken when it starts, so every read sees one consistent tip.

## Configuration

```toml
[utxo]
cache_mib = 512            # RocksDB block cache
```

## Not in v0.1

- In-memory coin cache with batched flushes.
- Parallel script verification.
- Policy script flags in `check_tx`.

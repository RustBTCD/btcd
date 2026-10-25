# Storage and consistency

## Stores

A **store** is the unit of atomicity: a set of tables written together in all-or-nothing updates.

- Data that MUST change atomically together MUST be in the same store.
- Each store uses one engine. The engine is chosen per store.
- A store belongs to exactly one daemon and lives in that daemon's data directory. No store is opened by more than one
  process.
- There is no atomicity across stores, within a daemon or between daemons. Consistency across stores is defined by
  [write ordering](#write-ordering).

### Engines

| Engine | Atomic unit | Durability |
|---|---|---|
| Key-value (RocksDB) | one write batch | a batch written with `sync` survives power loss; without it, it survives a process crash |
| Flat files (append-only) | one appended record | after `fsync` of the file |
| Single file | the whole file, replaced by writing a temporary file, `fsync`, `rename` | after the rename |

### Stores in v0.1

| Daemon | Store | Engine | Path |
|---|---|---|---|
| utxo | `chainstate` | RocksDB | `<datadir>/<network>/utxo/chainstate/` |
| chain | `index` | RocksDB | `<datadir>/<network>/chain/index/` |
| chain | `blocks` | flat files | `<datadir>/<network>/chain/blocks/` |
| net | `addresses` | single file | `<datadir>/<network>/net/addresses.json` |

`block-sync` has no stores.

### utxo

Store `chainstate`:

| Table | Key | Value |
|---|---|---|
| `coins` | outpoint | amount, scriptPubKey, height, coinbase flag |
| `undo` | block hash | the coins the block spent, in input order |
| `active` | height | block hash, block time, median time past |
| `meta` | `tip` | block hash of the tip; absent before genesis is connected |
| `meta` | `stats` | number of coins, total amount |

Every `utxo.connect_block` and `utxo.disconnect_block` is one write batch containing the coins added and removed, the
undo data, the `active` row, `tip` and `stats`.

v0.1 writes one batch per block and keeps no in-memory coin cache.

### chain

Store `index`:

| Table | Key | Value |
|---|---|---|
| `index` | block hash | header (80 bytes), height, cumulative chain work, status flags, block location (present when `have_data`) |
| `meta` | `block_files` | number of the current block file, write offset in it |

Status flags: `header_valid`, `have_data`, `block_valid` (passed `CheckBlock` and `ContextualCheckBlock`), `failed` (the
block itself is invalid), `failed_parent` (an ancestor is invalid). A block location is `{file, offset, length}` and
points at the block bytes inside a block file. The whole index is loaded into memory at startup.
The active chain is not stored by `chain` ([startup reconciliation](#startup-reconciliation)).

Store `blocks`:

- Files `blk00000.dat`, `blk00001.dat`, … in the store directory.
- Each record is the network magic (4 bytes), the block length (u32, little-endian), and the block in consensus
  encoding, as in Bitcoin Core's `blk*.dat` files.
- A file holds at most 128 MiB (Core's `MAX_BLOCKFILE_SIZE`). A record that does not fit starts the next file.
- Records are never modified or deleted in v0.1.

Storing a block body:

1. Append the record at the write position recorded in `meta.block_files`, then `fsync` the file.
2. Write one `index` batch with `sync`: the block's entry with `have_data` and its location, and `meta.block_files`
   advanced past the record.

Bytes beyond the recorded write position belong to no block. The next append overwrites them.

### net

Store `addresses`: one JSON file with the known addresses and, per address, services, last seen, last success and
attempt count. Rewritten every 15 minutes and at shutdown.

## The tip rule

`utxo`'s `meta.tip` in `chainstate` is the only persistent record of the active chain. `chain` holds the active chain in
memory and rebuilds it from `utxo` at startup. No other daemon persists the tip.

## Write ordering

Connecting a block:

1. `chain` stores the block body in `blocks` (appended and `fsync`ed).
2. `chain` writes the block's `index` entry with `sync`.
3. `chain` calls `utxo.connect_block`; `utxo` writes one `chainstate` batch. The batch does not need `sync`.
4. After `utxo` has answered, `chain` updates its in-memory active chain and emits `chain.block_connected` and
   `chain.tip_changed`.

Disconnecting a block: `chain` calls `utxo.disconnect_block` (the body is already stored);
after the answer it updates its in-memory active chain and emits `chain.block_disconnected`.

Invariant: every block in `utxo`'s active chain has a durable `index` entry with `have_data` and a durable body in
`blocks`.

## Startup reconciliation

During its initialisation (after `utxo` is ready, before `bus.ready`), `chain`:

1. Loads the `index` store into memory and determines the best header (most work, not failed).
   On the very first start it inserts the genesis header.
2. Calls `utxo.info` and reads `tip`.
3. If there is no tip, the active chain is empty. Genesis is connected during the first activation.
4. Otherwise the tip MUST be in the index with `have_data`. The active chain is the path from the tip back to genesis
   through the index. If the tip is not in the index, `chain` exits with a corruption error.
5. After `bus.start`, `chain` emits `chain.tip_changed` with the current tip and empty `connected`/`disconnected` lists,
   then runs best-chain activation, which connects stored blocks beyond the tip.

## Crash cases

| The node stops… | State on disk afterwards | Startup |
|---|---|---|
| during or after the append to `blocks`, before the `index` batch | bytes beyond the recorded write position | Ignored and overwritten by the next append; the block is downloaded again. |
| after the `index` batch, before `utxo` committed the block | `utxo` tip is the parent; the block is stored | Activation connects the block. |
| after `utxo` committed, before `chain` emitted events | `utxo` tip is the new block | Reconciliation makes it the active tip; the initial `chain.tip_changed` informs subscribers. |
| by power loss, after `utxo` acknowledged several blocks | `utxo` lost its last commits; `chain` has every block durably | `utxo`'s tip is an ancestor; activation connects the rest. |
| in the middle of a reorg | `utxo` tip is somewhere on the old or new branch | Reconciliation takes it as the tip; activation continues toward the most-work chain. |
| while `chain` stored headers | some headers missing | Downloaded again from peers. |
| while `net` rewrote `addresses` | the old or the new file | — |

Events are not persisted. A subscriber that missed events catches up from the initial `chain.tip_changed` and from RPCs
([handling rules](events-and-hooks.md#handling-rules-for-subscribers)).

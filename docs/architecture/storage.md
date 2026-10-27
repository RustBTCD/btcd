# Storage

## Introduction

A **store** is the unit of atomicity: a set of tables written together in all-or-nothing updates.

- Data that MUST change atomically together MUST be in the same store.
- Data that does not need to change atomically together is kept in separate stores, so that the engine of each store can
  be replaced independently.
- A store belongs to exactly one module.
- There is no atomicity across stores. Consistency across stores is defined by [write ordering](#write-ordering).

| Module | Store        | Tables                            | Path                                   |
|--------|--------------|-----------------------------------|----------------------------------------|
| chain  | `chainstate` | `coins`, `undo`, `active`, `meta` | `<datadir>/<network>/chain/chainstate/` |
| chain  | `headers`    | `headers`                         | `<datadir>/<network>/chain/headers/`   |
| chain  | `blocks`     | `blocks`                          | `<datadir>/<network>/chain/blocks/`    |
| net    | `peer-list`  | `peer-list`                       | `<datadir>/<network>/net/peer-list/`   |

`block-sync` has no store. Note that `headers` and `blocks` are separate stores: a block body and its header never need
to change together.

### Engine

Every store in v0.1 is a separate RocksDB instance in its own directory. Each table has a tuning profile:

| Profile      | Used for                           | Settings     |
|--------------|------------------------------------|--------------|
| point lookup | tables read by key                 | bloom filter |
| large values | tables of large values read by key | blob files   |
| sequential   | tables read in key order           | —            |

----

## Stores

## chainstate

| Table    | Key        | Value                                                                                   | Profile      |
|----------|------------|-----------------------------------------------------------------------------------------|--------------|
| `coins`  | outpoint   | amount, scriptPubKey, height, coinbase flag                                             | point lookup |
| `undo`   | block hash | the coins the block spent (amount, scriptPubKey, height, coinbase flag), in input order | large values |
| `active` | height     | block hash, block time and median time past of the active chain at that height          | sequential   |
| `meta`   | `tip`      | block hash and height of the tip                                                        | —            |

The UTXO set and the tip are in one store, so a block is applied or reverted together with the tip move.

## headers

| Table     | Key        | Value                                                    | Profile      |
|-----------|------------|----------------------------------------------------------|--------------|
| `headers` | block hash | header (80 bytes), height, cumulative chain work, status | point lookup |

## blocks

| Table    | Key        | Value                                               | Profile      |
|----------|------------|-----------------------------------------------------|--------------|
| `blocks` | block hash | block in consensus encoding, including witness data | large values |

## peer-list

| Table       | Key                      | Value                                                                  | Profile      |
|-------------|--------------------------|------------------------------------------------------------------------|--------------|
| `peer-list` | peer address (`ip:port`) | features recorded for the peer ([net](../modules/net/README.md#state)) | point lookup |

----

## Write ordering

> Note: We use 1,2,3... when the order is important and dots when not.
> When the order is important, we assume using RockDB write in sync mode `WriteOptions::set_sync(true)`.

### First start

- The `chain` writes the genesis: block to `blocks` and header to `headers`.
- The `chain` writes one `chainstate` batch, updating all involved tables (`meta`, `active`, `coins`, etc.).

### Storing headers

- Write by batches into `headers`

### Connecting a block to the tip block

1. `chain` writes the block to `blocks`.
2. `chain` writes one `chainstate` batch:
    - save new outputs to the `coins`;
    - remove spent outputs from `coins`, create corresponding `undo` for the block;
    - set `active` at the block's height;
    - set `meta.tip` = the block hash and height.

### Disconnecting the tip block

`chain` writes one `chainstate` batch:

- remove outputs created by the block from `coins`;
- save outputs from block's `undo` to `coins`;
- remove `undo` entry;
- remove height record from `active`;
- set `meta.tip` = its parent hash and height.

The disconnected block stays in `blocks`.


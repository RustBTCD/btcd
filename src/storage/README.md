# Storage

Persistence for the node: block bodies, headers, and the chainstate.

## Layers

| Module | Role |
| --- | --- |
| `db` | the interface: `Database`, `Batch`, `Store<T>`, the `Table` trait and its codecs |
| `engine` | implementations. Only this module names a storage engine |
| `tables` | every table, its key and value types, and how its data is used |
| `chain` | the chainstate group and the writes that move its tables together |

`Database` is byte-oriented and object safe, so a handle is an `Arc<dyn Database>` and nothing
above `engine` knows which engine is behind it. `Store<T>` binds one table to one handle and
adds the types back: `store.get(&key)` returns the table's value type.

## Databases

A database is opened with the tables it holds, and which table goes where is decided in code,
not in configuration. Today the node opens three:

| Database | Tables |
| --- | --- |
| `chainstate` | coins, undo, height index, meta |
| `blocks` | blocks |
| `headers` | headers |

Opening a `Store` checks that its database actually holds its table, so a wiring mistake fails
at startup rather than on the first write.

## Tables

| Table | Key | Value | Why |
| --- | --- | --- | --- |
| `coins` | outpoint | amount, script, creation height, coinbase flag | the unspent output set, which is what validation spends from |
| `undo` | block hash | the coins that block spent | a block deletes coins when it connects; this is the only copy left, and disconnecting restores from it |
| `height_index` | height | block hash | which block the active chain has at a height |
| `meta` | fixed keys | block hash | the tip: the block whose state the coin set represents |
| `blocks` | block hash | the block | bodies as received, on any branch |
| `headers` | block hash | header, height, cumulative work, status | the header tree, loaded at startup |

Key encodings that matter: an outpoint is the transaction id followed by the output index in
big-endian order, so one transaction's outputs sit together and a scan by transaction id
answers whether it still has unspent outputs. Heights are big-endian so a scan walks the chain
in order.

A block's own outputs are not stored anywhere separately: disconnecting recomputes them from
the block. Only the spent side needs saving, because a block carries references to the coins it
spends, not their contents. Bitcoin Core keeps the same split in its undo files.

## Writing

Every write goes through a `Batch`, which an engine applies in one step. A batch cannot span
databases, so:

- Tables that must move together share a database. That is why coins, undo, the height index
  and the tip are one group, written by `Chainstate::write`, which hands out a transaction that
  reaches only those tables.
- Anything else commits separately, and order is what keeps the result consistent: store a
  block body before the chainstate that references it, and move the tip last. A crash then
  leaves at worst a stored block nothing points at, which is what an unconnected block already
  looks like.

## Adding a table

1. Declare it in `tables.rs`: a marker type, its name, how its data is used, and its key, value
   and prefix types.
2. Give its value type the byte codecs, unless it already has them.
3. Name it where the database that should hold it is opened.

Step three is the one the compiler cannot check: a table nobody opens does not exist, and the
store that wants it fails at startup with that table's name.

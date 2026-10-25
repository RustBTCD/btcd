# utxo RPCs

Requests and responses are defined by the schemas in [schemas/rpc/](schemas/rpc/); shared types are in
[common.json](../../schemas/common.json).

| RPC | Operator | Kind | Schema |
|---|---|---|---|
| [`utxo.info`](#utxoinfo) | ✓ | read | [info.json](schemas/rpc/info.json) |
| [`utxo.coin`](#utxocoin) | ✓ | read | [coin.json](schemas/rpc/coin.json) |
| [`utxo.connect_block`](#utxoconnect_block) | | write | [connect_block.json](schemas/rpc/connect_block.json) |
| [`utxo.disconnect_block`](#utxodisconnect_block) | | write | [disconnect_block.json](schemas/rpc/disconnect_block.json) |

Only `chain` calls the write RPCs.

## `utxo.info`

UTXO tip and set statistics. `tip` and `mtp` are `null` before genesis is connected.

## `utxo.coin`

One unspent output, as Core's `gettxout` without mempool. `address` is present only when the script has a standard
address form on the node's network.

Errors: `not_found` if the output does not exist or is spent.

## `utxo.connect_block`

Validate a block's inputs and scripts and apply it on top of the tip. Atomic. The block's parent must be the current tip
(or, at height 0, there must be no tip). `chain` decides the rules (`script_flags`, `verify_scripts`, `enforce_bip30`,
`enforce_bip68`); `utxo` applies them.

Errors:

- `invalid`, with `data.reason` from Core where one exists: `bad-txns-BIP30`, `bad-txns-inputs-missingorspent`,
  `bad-txns-premature-spend-of-coinbase`, `bad-txns-in-belowout`, `bad-txns-inputvalues-outofrange`, `bad-txns-nonfinal`
  (sequence locks), `bad-blk-sigops`, `bad-cb-amount`, `block-validation-failed` (with `data.tx` and `data.input` for
  script failures). Nothing was changed.
- `precondition_failed`: the block does not extend the tip, or `height` is wrong. A bug in `chain`.

## `utxo.disconnect_block`

Undo the tip block using its undo data. Atomic. The block's outputs identify the coins to remove.

Errors:

- `precondition_failed`: the block is not the tip.
- `internal_error`: undo data is missing or does not match the block. The database is corrupt; `chain` stops the node.

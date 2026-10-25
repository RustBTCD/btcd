# utxo RPCs

Types are defined in [types.md](../../architecture/types.md).

| RPC | Operator | Callers | Kind |
|---|---|---|---|
| [`utxo.info`](#utxoinfo) | ✓ | chain | read |
| [`utxo.coin`](#utxocoin) | ✓ | | read |
| [`utxo.connect_block`](#utxoconnect_block) | | chain | write |
| [`utxo.disconnect_block`](#utxodisconnect_block) | | chain | write |

The write RPCs are callable only by `chain`.

## `utxo.info`

Params: none.

```json
{
  "tip": {"hash": "<Hash>", "height": 850000},
  "mtp": 1717998000,
  "coins": 180000000,
  "total_amount": 1971234500000000,
  "size_on_disk": 11000000000
}
```

`tip` and `mtp` are `null` before genesis is connected.

## `utxo.coin`

One unspent output, as Core's `gettxout` without mempool.

| Param | Type | Required | Description |
|---|---|---|---|
| `txid` | string | yes | Hash |
| `vout` | integer | yes | Output index |

```json
{
  "outpoint": {"txid": "<Hash>", "vout": 1},
  "amount": 150000,
  "script_pubkey": "<Hex>",
  "address": "bc1q…",
  "height": 849000,
  "coinbase": false,
  "confirmations": 1001,
  "best_block": {"hash": "<Hash>", "height": 850000}
}
```

Errors: `not_found` if the output does not exist or is spent.

## `utxo.connect_block`

Validate a block's inputs and scripts and apply it on top of the tip. Atomic.

| Param | Type | Required | Description |
|---|---|---|---|
| `block` | string | yes | BlockHex |
| `height` | integer | yes | Its height |
| `script_flags` | [string] | yes | Consensus script flags for this block: any of `P2SH`, `DERSIG`, `NULLDUMMY`, `CHECKLOCKTIMEVERIFY`, `CHECKSEQUENCEVERIFY`, `WITNESS`, `TAPROOT` |
| `verify_scripts` | boolean | yes | `false` when assumevalid applies |
| `enforce_bip30` | boolean | yes | Whether to check BIP30 for this block |
| `enforce_bip68` | boolean | yes | Whether BIP68 sequence locks apply (CSV active) |

The block's parent must be the current tip (or, at height 0, there must be no tip).

```json
{"tip": {"hash": "<Hash>", "height": 850001}, "fees": 12345678}
```

Errors:

- `invalid`, with `data.reason` from Core where one exists: `bad-txns-BIP30`, `bad-txns-inputs-missingorspent`,
  `bad-txns-premature-spend-of-coinbase`, `bad-txns-in-belowout`, `bad-txns-inputvalues-outofrange`, `bad-txns-nonfinal`
  (sequence locks), `bad-blk-sigops`, `bad-cb-amount`, `block-validation-failed` (with `data.tx` and `data.input` for
  script failures). Nothing was changed.
- `precondition_failed`: the block does not extend the tip, or `height` is wrong. A bug in `chain`.

## `utxo.disconnect_block`

Undo the tip block. Atomic.

| Param | Type | Required | Description |
|---|---|---|---|
| `block` | string | yes | BlockHex of the current tip; its outputs identify the coins to remove |

```json
{"tip": {"hash": "<Hash>", "height": 849999}}
```

Errors:

- `precondition_failed`: the block is not the tip.
- `internal_error`: undo data is missing or does not match the block. The database is corrupt; `chain` stops the node.

# chain RPCs

Types are defined in [types.md](../../architecture/types.md).

| RPC | Operator | Callers | Kind |
|---|---|---|---|
| [`chain.info`](#chaininfo) | ✓ | block-sync, tx-relay | read |
| [`chain.block`](#chainblock) | ✓ | block-sync | read |
| [`chain.header`](#chainheader) | ✓ | | read |
| [`chain.tips`](#chaintips) | ✓ | | read |
| [`chain.tx`](#chaintx) | ✓ | | read |
| [`chain.submit_headers`](#chainsubmit_headers) | | block-sync | write |
| [`chain.submit_block`](#chainsubmit_block) | | block-sync | write |
| [`chain.locator`](#chainlocator) | | block-sync | read |
| [`chain.missing_blocks`](#chainmissing_blocks) | | block-sync | read |
| [`chain.headers_after`](#chainheaders_after) | | block-sync | read |

## `chain.info`

Params: none.

```json
{
  "network": "main",
  "tip": {"hash": "<Hash>", "height": 850000, "time": 1718000000, "mtp": 1717998000, "work": "<Work>"},
  "best_header": {"hash": "<Hash>", "height": 850002, "work": "<Work>"},
  "ibd": false,
  "assume_valid": "<Hash or null>",
  "min_chain_work": "<Work>",
  "headers": 850002,
  "size_on_disk": 650000000000
}
```

`tip` is `null` before genesis is connected. `headers` counts valid headers in the index.

## `chain.block`

| Param | Type | Required | Description |
|---|---|---|---|
| `block` | string | yes | BlockId: hash, or height on the active chain |
| `verbosity` | integer | no | `0` raw, `1` decoded (default) |

Verbosity 0:

```json
{"hash": "<Hash>", "height": 850000, "in_active_chain": true, "hex": "<BlockHex>"}
```

Verbosity 1: every field of `HeaderInfo`, plus:

```json
{"size": 1534567, "stripped_size": 812345, "weight": 3991234, "tx_count": 3456, "txids": ["<Hash>"]}
```

`block-sync` uses verbosity 0 to serve `getdata` and serves only blocks with `in_active_chain: true`.

Errors: `not_found` with `data.reason` `unknown-block` or `block-data-missing` (header known, body not stored).

## `chain.header`

| Param | Type | Required | Description |
|---|---|---|---|
| `block` | string | yes | BlockId |

Returns `HeaderInfo` plus `"hex": HeaderHex`. Works for any header in the index, including ones whose block is not stored.

Errors: `not_found`.

## `chain.tips`

Params: none. Same meaning as Core's `getchaintips`.

```json
{
  "tips": [
    {"hash": "<Hash>", "height": 850000, "branch_len": 0, "status": "active"},
    {"hash": "<Hash>", "height": 849990, "branch_len": 1, "status": "valid_fork"}
  ]
}
```

`status`: `active`, `valid_fork`, `valid_headers`, `headers_only`, `invalid`.

## `chain.tx`

Find a transaction in a given block. Without a transaction index, the block must be named (as Core's `getrawtransaction` with a block hash).

| Param | Type | Required | Description |
|---|---|---|---|
| `txid` | string | yes | Hash |
| `block` | string | yes | BlockId |

Returns `TxInfo` plus:

```json
{"block": {"hash": "<Hash>", "height": 850000}, "index": 17, "confirmations": 3}
```

`confirmations` is `0` if the block is not on the active chain.

Errors: `not_found` with `data.reason` `unknown-block`, `block-data-missing` or `tx-not-in-block`.

## `chain.submit_headers`

Validate and store headers received from a peer. Processing stops at the first invalid header; headers before it are kept.

| Param | Type | Required | Description |
|---|---|---|---|
| `headers` | [string] | yes | HeaderHex list in chain order, each extending the previous; the first extends a known header |
| `peer` | integer | no | PeerId, for logs |

```json
{
  "status": "ok",
  "new": 2000,
  "last": {"hash": "<Hash>", "height": 2000},
  "best_header": {"hash": "<Hash>", "height": 2000, "work": "<Work>"}
}
```

| `status` | Meaning |
|---|---|
| `ok` | Every header is valid. `new` counts headers that were not known before. `last` is the last header of the list. |
| `unconnected` | The first header's parent is unknown. Nothing was stored. The caller should send `getheaders` with a fresh locator. |

Errors: `invalid` with `data` `{"reason": <Core reason>, "hash": Hash, "index": <position in list>}`; for example `high-hash`, `bad-diffbits`, `time-too-old`, `time-too-new`, `bad-version`, `bad-prevblk`. Also `invalid_params` for headers that do not chain to each other.

## `chain.submit_block`

| Param | Type | Required | Description |
|---|---|---|---|
| `block` | string | yes | BlockHex |
| `requested` | boolean | yes | Whether block-sync asked for this block |
| `peer` | integer | no | PeerId, for logs |

```json
{"status": "connected", "hash": "<Hash>", "tip": {"hash": "<Hash>", "height": 850001}}
```

| `status` | Meaning |
|---|---|
| `connected` | Stored and now on the active chain. |
| `stored` | Stored, not (yet) on the active chain: a gap before it, or a fork with less work. |
| `duplicate` | Already stored. |
| `unconnected` | The block's parent header is unknown. |
| `ignored` | Unrequested and not useful ([README](README.md#block-submission) step 4). |

Errors: `invalid` with `data` `{"reason": <Core reason>, "mutated": bool}`. For example `bad-txnmrklroot` (mutated), `bad-cb-missing`, `bad-blk-weight`, `bad-txns-inputs-missingorspent` (from `utxo`), `block-validation-failed` (scripts). The error refers to the submitted block: it failed itself, or an ancestor connected in the same activation failed and it is now marked as having an invalid parent. Failures of unrelated blocks are only logged.

## `chain.locator`

| Param | Type | Required | Description |
|---|---|---|---|
| `from` | string | no | Hash to start from; default the best header |

Returns `{"locator": [Hash]}`: the starting hash, then the previous 10 hashes, then exponentially larger steps back to genesis (Core's `GetLocator`). Error: `not_found`.

## `chain.missing_blocks`

Blocks block-sync should download next.

| Param | Type | Required | Description |
|---|---|---|---|
| `limit` | integer | no | Maximum entries, default 1024 |
| `window` | integer | no | Only blocks at most this many heights above the active tip, default 1024 |

Returns `{"blocks": [BlockRef]}` in ascending height: the blocks on the most-work valid header chain, above the fork point with the active chain, whose bodies are not stored, within the window. Empty when the active tip is the best header.

## `chain.headers_after`

Serve a peer's `getheaders` from the active chain, as Core does.

| Param | Type | Required | Description |
|---|---|---|---|
| `locator` | [string] | yes | Hashes |
| `stop` | string | no | Hash; stop after this header |
| `max` | integer | no | Default and maximum 2000 |

Starting point: the first locator hash that is on the active chain, or genesis if none is. Returns `{"headers": [HeaderHex]}` that follow it on the active chain. An empty locator returns only the `stop` header, if known.

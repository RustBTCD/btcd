# chain RPCs

Requests and responses are defined by the schemas in [schemas/rpc/](schemas/rpc/); shared types are in
[schemas/types.json](schemas/types.json) and [common.json](../../schemas/common.json).

| RPC | Operator | Kind | Schema |
|---|---|---|---|
| [`chain.info`](#chaininfo) | ✓ | read | [info.json](schemas/rpc/info.json) |
| [`chain.block`](#chainblock) | ✓ | read | [block.json](schemas/rpc/block.json) |
| [`chain.raw_block`](#chainraw_block) | ✓ | read | [raw_block.json](schemas/rpc/raw_block.json) |
| [`chain.header`](#chainheader) | ✓ | read | [header.json](schemas/rpc/header.json) |
| [`chain.tips`](#chaintips) | ✓ | read | [tips.json](schemas/rpc/tips.json) |
| [`chain.tx`](#chaintx) | ✓ | read | [tx.json](schemas/rpc/tx.json) |
| [`chain.submit_headers`](#chainsubmit_headers) |  | write | [submit_headers.json](schemas/rpc/submit_headers.json) |
| [`chain.submit_block`](#chainsubmit_block) |  | write | [submit_block.json](schemas/rpc/submit_block.json) |
| [`chain.locator`](#chainlocator) |  | read | [locator.json](schemas/rpc/locator.json) |
| [`chain.missing_blocks`](#chainmissing_blocks) |  | read | [missing_blocks.json](schemas/rpc/missing_blocks.json) |
| [`chain.headers_after`](#chainheaders_after) |  | read | [headers_after.json](schemas/rpc/headers_after.json) |

## `chain.info`

Tip, best header and IBD state. `tip` is `null` before genesis is connected. `headers` counts valid headers in the
index.

## `chain.block`

A stored block, decoded: its `HeaderInfo`, sizes, weight and transaction ids. `chain.raw_block` returns the block
itself.

Errors: `not_found` with `data.reason` `unknown-block` or `block-data-missing` (header known, body not stored).

## `chain.raw_block`

A stored block in consensus encoding, with its hash, height and whether it is on the active chain. `block-sync` uses it
to serve `getdata` and serves only blocks with `in_active_chain: true`.

Errors: `not_found` with `data.reason` `unknown-block` or `block-data-missing`.

## `chain.header`

A header's `HeaderInfo` and its encoding. Works for any header in the index, including ones whose block is not stored.

Errors: `not_found`.

## `chain.tips`

All known chain tips, with the same meaning as Core's `getchaintips`.

## `chain.tx`

A transaction from a given block, with the block, its position and its confirmations. Without a transaction index, the
block must be named (as Core's `getrawtransaction` with a block hash). `confirmations` is `0` if the block is not on
the active chain.

Errors: `not_found` with `data.reason` `unknown-block`, `block-data-missing` or `tx-not-in-block`.

## `chain.submit_headers`

Validate and store headers received from a peer. Processing stops at the first invalid header; headers before it are
kept.

- `ok`: every header is valid. `new` counts headers that were not known before; `last` is the last header of the list.
- `unconnected`: the first header's parent is unknown. Nothing was stored, and `last` is absent. The caller should send
  `getheaders` with a fresh locator.

Errors: `invalid` with `data` `{"reason": <Core reason>, "hash": Hash, "index": <position in list>}`;
for example `high-hash`, `bad-diffbits`, `time-too-old`, `time-too-new`, `bad-version`, `bad-prevblk`.
Also `invalid_params` for headers that do not chain to each other.

## `chain.submit_block`

Validate, store and possibly connect a block. `status` is one of:

| `status` | Meaning |
|---|---|
| `connected` | Stored and now on the active chain. |
| `stored` | Stored, not (yet) on the active chain: a gap before it, or a fork with less work. |
| `duplicate` | Already stored. |
| `unconnected` | The block's parent header is unknown. |
| `ignored` | Unrequested and not useful ([README](README.md#block-submission) step 4). |

Errors: `invalid` with `data` `{"reason": <Core reason>, "mutated": bool}`. For example `bad-txnmrklroot` (mutated),
`bad-cb-missing`, `bad-blk-weight`, `bad-txns-inputs-missingorspent` (from `utxo`), `block-validation-failed` (scripts).
The error refers to the submitted block: it failed itself, or an ancestor connected in the same activation failed and it
is now marked as having an invalid parent. Failures of unrelated blocks are only logged.

## `chain.locator`

A block locator: the starting hash (default the best header), then the previous 10 hashes, then exponentially larger
steps back to genesis (Core's `GetLocator`).

Errors: `not_found`.

## `chain.missing_blocks`

Blocks block-sync should download next, in ascending height: the blocks on the most-work valid header chain, above the
fork point with the active chain, whose bodies are not stored, within the window above the active tip. Empty when the
active tip is the best header.

## `chain.headers_after`

Serve a peer's `getheaders` from the active chain, as Core does. Starting point: the first locator hash that is on the
active chain, or genesis if none is. Returns the headers that follow it on the active chain, up to `stop` or `max`.
An empty locator returns only the `stop` header, if known.

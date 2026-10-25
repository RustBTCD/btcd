# Common types

How values are represented in JSON on the bus and the operator socket. Event and RPC schemas in the daemon folders use
these names.

## Scalars

| Name | JSON | Format |
|---|---|---|
| `Hash` | string | 64 lowercase hex characters, in the byte order Bitcoin Core displays (reversed). Used for block hashes, txids and wtxids. |
| `Hex` | string | Lowercase hex of bytes, no `0x` prefix. |
| `Height` | number | u32. |
| `PeerId` | number | u64 assigned by `net`, unique for the lifetime of the `net` process and never reused. |
| `Amount` | number | i64 satoshis. Never BTC as a decimal. |
| `Work` | string | Cumulative chain work as 64 hex characters, as Core's `chainwork`. |
| `Timestamp` | number | Unix time in seconds. |
| `Network` | string | `main`, `testnet4`, `signet`, `regtest`. |
| `Services` | object | `{"bits": u64, "names": [string]}`, names as Core prints them (`NETWORK`, `WITNESS`, `NETWORK_LIMITED`, `P2P_V2`, …). |
| `BlockId` | string | A `Hash`, or a decimal height on the active chain. Parsed by the callee: 64 hex characters is a hash, digits is a height. |

## Bitcoin objects

| Name | JSON | Format |
|---|---|---|
| `HeaderHex` | string | `Hex` of the 80-byte header in consensus encoding. |
| `BlockHex` | string | `Hex` of the block in consensus encoding, with witness data. |
| `TxHex` | string | `Hex` of the transaction in consensus encoding, with witness data if it has any. |

## Structured types

### `Outpoint`

```json
{"txid": "<Hash>", "vout": 0}
```

### `BlockRef`

```json
{"hash": "<Hash>", "height": 850000}
```

### `HeaderInfo`

What `chain` knows about a header.

```json
{
  "hash": "<Hash>",
  "height": 850000,
  "prev": "<Hash>",
  "version": 536870912,
  "merkle_root": "<Hash>",
  "time": 1718000000,
  "mtp": 1717998000,
  "bits": "17034219",
  "nonce": 123456,
  "work": "<Work>",
  "status": "active",
  "have_data": true,
  "confirmations": 3,
  "next": "<Hash or null>"
}
```

`status` is one of `active` (on the active chain), `valid_fork` (fully validated, not on the active chain),
`valid_headers` (block stored and checked by `chain`, never connected), `headers_only` (block not stored), `invalid`.
The same values are used by `chain.tips`, matching Core's `getchaintips`. `confirmations` and `next` are relative to the
active chain, `0` and `null` when not on it. `bits` is the compact target as 8 hex characters.

### `TxInfo`

A decoded transaction.

```json
{
  "txid": "<Hash>",
  "wtxid": "<Hash>",
  "version": 2,
  "locktime": 0,
  "size": 222,
  "vsize": 141,
  "weight": 561,
  "inputs": [
    {
      "prevout": {"txid": "<Hash>", "vout": 1},
      "script_sig": "<Hex>",
      "sequence": 4294967293,
      "witness": ["<Hex>", "<Hex>"]
    }
  ],
  "outputs": [
    {"n": 0, "amount": 150000, "script_pubkey": "<Hex>", "address": "bc1q…"}
  ],
  "hex": "<TxHex>"
}
```

For a coinbase input, `prevout` is `null` and `coinbase` holds the script as `Hex` instead of `script_sig`.
`address` is present only when the script has a standard address form on the node's network.

### `PeerMessage`

The payload of `net.send`; see [net/rpc.md](../daemons/net/rpc.md#netsend).

## Rules

- Field names are snake_case.
- Optional fields are omitted when absent, except where a schema shows `null` explicitly.
- Receivers MUST reject unknown fields in RPC params (`invalid_params`). Receivers MUST ignore unknown fields in events
  and results.

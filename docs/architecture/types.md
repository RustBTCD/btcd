# Common types

How values are represented in JSON on the bus and the operator socket. Event and RPC schemas in the daemon folders use
these names. Their exact definitions are in `docs/schemas/common.json` ([schemas.md](schemas.md)).

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

### Types owned by one daemon

Types used by only one daemon are defined in that daemon's `types.json`:

- `HeaderInfo`, `TxInfo`, `TxInput`, `TxOutput`: [chain/schemas/types.json](../daemons/chain/schemas/types.json).
- `PeerMessage`, `InvItem`, `ConnType`: [net/schemas/types.json](../daemons/net/schemas/types.json).

## Rules

- Field names are snake_case.
- Optional fields are omitted when absent, except where a schema shows `null` explicitly.
- Receivers MUST reject unknown fields in RPC params (`invalid_params`). Receivers MUST ignore unknown fields in events
  and results.

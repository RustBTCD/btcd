# tx-relay RPCs

Types are defined in [types.md](../../architecture/types.md).

| RPC | Operator | Callers |
|---|---|---|
| [`tx-relay.info`](#tx-relayinfo) | ✓ | |
| [`tx-relay.tx`](#tx-relaytx) | ✓ | |
| [`tx-relay.list`](#tx-relaylist) | ✓ | |
| [`tx-relay.submit`](#tx-relaysubmit) | ✓ | |

## `tx-relay.info`

Params: none.

```json
{
  "ibd": false,
  "txs": 4210,
  "bytes": 2345678,
  "max_bytes": 67108864,
  "min_relay_feerate": 100,
  "accepted_total": 120345,
  "rejected_total": 2345,
  "relaying_peers": 18
}
```

`accepted_total` and `rejected_total` count since start.

## `tx-relay.tx`

An unconfirmed transaction from the relay cache. For confirmed transactions, use `chain.tx`.

| Param | Type | Required | Description |
|---|---|---|---|
| `id` | string | yes | Txid or wtxid |

Returns `TxInfo` plus:

```json
{"fee": 2820, "feerate": 20000, "received_at": 1718000000, "peer": 7}
```

`feerate` is in sat/kvB. `peer` is `null` for operator submissions.

Errors: `not_found`.

## `tx-relay.list`

Params: none. Returns `{"txids": [Hash]}`, highest fee rate first.

## `tx-relay.submit`

Validate a transaction, add it to the relay cache and announce it to every relaying peer (Core's `sendrawtransaction`, without the max fee rate check).

| Param | Type | Required | Description |
|---|---|---|---|
| `tx` | string | yes | TxHex |

```json
{"txid": "<Hash>", "wtxid": "<Hash>", "fee": 2820, "vsize": 141}
```

Submitting a transaction that is already cached returns the same result again.

Errors:

- `invalid_params`: the hex does not decode as a transaction.
- `missing_inputs`: an input is unconfirmed or does not exist; `data.missing` lists them.
- `invalid`: consensus failure, `data.reason` from `utxo.check_tx`.
- `rejected`: policy (`tx-size`, `bad-txns-too-many-sigops`, `min relay fee not met`, `txn-mempool-conflict`, `mempool full`) or `rejected by hook: <reason>`.
- `rejected` with `data.reason` `initial-block-download`: the UTXO set is not current, so the transaction cannot be validated yet.

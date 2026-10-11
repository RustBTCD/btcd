# tx-relay events

Types are defined in [types.md](../../architecture/types.md). No v0.1 daemon subscribes to these events.

## `tx-relay.tx_accepted`

A transaction entered the relay cache.

| Field | Type | Description |
|---|---|---|
| `txid` | Hash | |
| `wtxid` | Hash | |
| `fee` | Amount | |
| `vsize` | number | |
| `peer` | PeerId or null | Source peer; `null` for an operator submission |

## `tx-relay.tx_removed`

A transaction left the relay cache.

| Field | Type | Description |
|---|---|---|
| `txid` | Hash | |
| `wtxid` | Hash | |
| `reason` | string | `confirmed_or_conflicted`, `expired`, `evicted` |

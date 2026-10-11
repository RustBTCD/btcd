# chain events

Types are defined in [types.md](../../architecture/types.md).

Order of emission during one activation: every `chain.block_disconnected`, then every `chain.block_connected`, then `chain.tip_changed`. Each event is emitted only after `utxo` has committed the change it describes.

## `chain.block_connected`

A block was connected to the active chain.

| Field | Type | Description |
|---|---|---|
| `hash` | Hash | |
| `height` | Height | |
| `prev` | Hash | |

Subscribers in v0.1: none.

## `chain.block_disconnected`

A block was disconnected from the tip of the active chain during a reorg.

| Field | Type | Description |
|---|---|---|
| `hash` | Hash | |
| `height` | Height | |

Subscribers in v0.1: none.

## `chain.tip_changed`

The active tip moved. Emitted once after `bus.start` with the current tip and empty lists, then after every activation that changed the tip ([README](README.md#best-chain-activation)).

| Field | Type | Description |
|---|---|---|
| `tip` | BlockRef | The new tip. `null` only if not even genesis is connected yet. |
| `connected` | [object] | Blocks on the new active chain that were not on the previous one, ascending height: `{"hash": Hash, "height": Height, "header": HeaderHex}`. At most 128; longer runs are split into several events. |
| `disconnected` | [BlockRef] | Blocks on the previous active chain that are no longer on it, from the old tip downwards. |
| `ibd` | bool | Whether the node is in initial block download. |

Subscribers:

| Subscriber | Uses it to |
|---|---|
| net | Fill the start height of new `version` messages. |
| block-sync | Announce new blocks to peers (only when `ibd` is false), refill the download window. |
| tx-relay | Turn relay on or off with `ibd`; drop cached transactions that were confirmed or conflicted. |

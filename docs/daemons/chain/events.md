# chain events

Payloads are defined by the schemas in [schemas/events/](schemas/events/); shared types are in
[schemas/types.json](schemas/types.json) and [common.json](../../schemas/common.json).

Order of emission during one activation: every `chain.block_disconnected`, then every `chain.block_connected`, then
`chain.tip_changed`. Each event is emitted only after `utxo` has committed the change it describes.

| Event | Schema | Subscribers | Meaning |
|---|---|---|---|
| `chain.block_connected` | [block_connected.json](schemas/events/block_connected.json) | — | A block was connected to the active chain. |
| `chain.block_disconnected` | [block_disconnected.json](schemas/events/block_disconnected.json) | — | A block was disconnected from the tip of the active chain during a reorg. |
| `chain.tip_changed` | [tip_changed.json](schemas/events/tip_changed.json) | net, block-sync | The active tip moved. |

## `chain.tip_changed`

Emitted once after `bus.start` with the current tip and empty lists, then after every activation that changed the tip
([README](README.md#best-chain-activation)). `connected` lists at most 128 blocks; longer runs are split into several
events.

| Subscriber | Uses it to |
|---|---|
| net | Fill the start height of new `version` messages. |
| block-sync | Announce new blocks to peers (only when `ibd` is false), refill the download window. |

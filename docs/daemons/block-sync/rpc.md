# block-sync RPCs

Requests and responses are defined by the schemas in [schemas/rpc/](schemas/rpc/); shared types are in
[common.json](../../schemas/common.json).

| RPC | Operator | Schema |
|---|---|---|
| [`block-sync.status`](#block-syncstatus) | ✓ | [status.json](schemas/rpc/status.json) |

## `block-sync.status`

Sync state, peers and blocks in flight.

| `state` | Meaning |
|---|---|
| `headers_sync` | Waiting for headers from the sync peer; nothing to download yet. |
| `block_download` | Blocks between the tip and the best header are being downloaded. |
| `synced` | The tip is the best header and the node is out of IBD. |

`tip` and `best_header` are block-sync's view from the latest `chain.tip_changed` and `chain.submit_headers` results;
`chain.info` is authoritative. `sync_peer` is `null` outside IBD.

# block-sync RPCs

| RPC | Operator | Callers |
|---|---|---|
| [`block-sync.status`](#block-syncstatus) | ✓ | |

## `block-sync.status`

Params: none.

```json
{
  "state": "block_download",
  "ibd": true,
  "tip": {"hash": "<Hash>", "height": 512000},
  "best_header": {"hash": "<Hash>", "height": 850000},
  "sync_peer": 7,
  "in_flight": 128,
  "stall_timeout_secs": 4,
  "peers": [
    {"peer": 7, "best_known_height": 850000, "in_flight": 16, "prefers_headers": true, "serving_queue": 0}
  ]
}
```

| `state` | Meaning |
|---|---|
| `headers_sync` | Waiting for headers from the sync peer; nothing to download yet. |
| `block_download` | Blocks between the tip and the best header are being downloaded. |
| `synced` | The tip is the best header and the node is out of IBD. |

`tip` and `best_header` are block-sync's view from the latest `chain.tip_changed` and `chain.submit_headers` results; `chain.info` is authoritative. `sync_peer` is `null` outside IBD.

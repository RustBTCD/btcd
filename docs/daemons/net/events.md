# net events

Payloads are defined by the schemas in [schemas/events/](schemas/events/); shared types are in
[schemas/types.json](schemas/types.json) and [common.json](../../schemas/common.json).

Every event except `net.peer_connected` and `net.peer_disconnected` carries the decoded content of one P2P message (or
the part of it routed to the subscriber). For a given peer, events are emitted in the order its messages arrived, after
`net.peer_connected` and before `net.peer_disconnected`.

| Event | Schema | Subscribers | Meaning |
|---|---|---|---|
| `net.peer_connected` | [peer_connected.json](schemas/events/peer_connected.json) | block-sync | The handshake completed (both `verack`s exchanged) and `hook.peer_connected` did not reject the peer. |
| `net.peer_disconnected` | [peer_disconnected.json](schemas/events/peer_disconnected.json) | block-sync | The connection closed. Last event about this peer. |
| `net.headers` | [headers.json](schemas/events/headers.json) | block-sync | A `headers` message. |
| `net.inv_blocks` | [inv_blocks.json](schemas/events/inv_blocks.json) | block-sync | The block items of an `inv` message. |
| `net.block` | [block.json](schemas/events/block.json) | block-sync | A `block` message. |
| `net.getheaders` | [getheaders.json](schemas/events/getheaders.json) | block-sync | A `getheaders` message. |
| `net.getdata_blocks` | [getdata_blocks.json](schemas/events/getdata_blocks.json) | block-sync | The block items of a `getdata` message. |
| `net.sendheaders` | [sendheaders.json](schemas/events/sendheaders.json) | block-sync | The peer prefers block announcements as `headers` (BIP130). |

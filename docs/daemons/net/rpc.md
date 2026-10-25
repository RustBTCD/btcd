# net RPCs

Types are defined in [types.md](../../architecture/types.md). `InvItem` is defined in [events.md](events.md).

| RPC | Operator | One-way | Callers |
|---|---|---|---|
| [`net.info`](#netinfo) | ✓ | | |
| [`net.peers`](#netpeers) | ✓ | | |
| [`net.connect`](#netconnect) | ✓ | | |
| [`net.disconnect`](#netdisconnect) | ✓ | | block-sync |
| [`net.send`](#netsend) | | ✓ | block-sync |
| [`net.misbehaving`](#netmisbehaving) | | ✓ | block-sync |

## `net.info`

Params: none.

```json
{
  "listening": true,
  "local_addresses": ["0.0.0.0:8333", "[::]:8333"],
  "protocol_version": 70016,
  "user_agent": "/rbtc:0.1.0/",
  "services": {"bits": 1033, "names": ["NETWORK", "WITNESS", "NETWORK_LIMITED"]},
  "connections": {"inbound": 12, "outbound": 8, "manual": 0},
  "known_addresses": 15234,
  "bytes_sent": 123456789,
  "bytes_received": 987654321,
  "paused": false
}
```

## `net.peers`

Params: none.

```json
{
  "peers": [
    {
      "peer": 7,
      "addr": "203.0.113.5:8333",
      "inbound": false,
      "conn_type": "outbound_full_relay",
      "services": {"bits": 3081, "names": ["NETWORK", "WITNESS", "NETWORK_LIMITED", "P2P_V2"]},
      "version": 70016,
      "user_agent": "/Satoshi:31.0.0/",
      "start_height": 850000,
      "relay": true,
      "connected_at": 1718000000,
      "last_send": 1718000500,
      "last_recv": 1718000501,
      "bytes_sent": 12345,
      "bytes_received": 67890,
      "ping_ms": 42
    }
  ]
}
```

`ping_ms` is `null` until the first `pong`.

## `net.connect`

Open a manual connection. It is reconnected if lost, until the node restarts.

| Param | Type | Required | Description |
|---|---|---|---|
| `addr` | string | yes | `host:port`; host may be a name |

Returns `{"peer": PeerId}` once the handshake completes.

Errors: `invalid_params` (cannot parse or resolve), `rejected` with `data.reason` (`connection refused`, `timeout`,
`handshake failed`, `rejected by hook: <reason>`, `already connected`).

## `net.disconnect`

| Param | Type | Required | Description |
|---|---|---|---|
| `peer` | integer | yes | PeerId |
| `reason` | string | no | Logged; default `requested` |

Returns `{}`. A manual peer disconnected this way is not reconnected.

Errors: `not_found`.

## `net.send`

One-way. Queue a message for a peer. Silently dropped if the peer is gone, which is normal:
the caller may not have processed `net.peer_disconnected` yet.

| Param | Type | Required | Description |
|---|---|---|---|
| `peer` | integer | yes | PeerId |
| `message` | PeerMessage | yes | See below |

`PeerMessage` is one of:

```json
{"type": "getheaders", "locator": ["<Hash>"], "stop": null}
{"type": "headers",    "headers": ["<HeaderHex>"]}
{"type": "inv",        "items": [{"type": "witness_block", "hash": "<Hash>"}]}
{"type": "getdata",    "items": [{"type": "witness_block", "hash": "<Hash>"}]}
{"type": "block",      "block": "<BlockHex>", "witness": true}
```

- For `block`, `witness: false` makes `net` serialize without witness data (BIP144), for peers that asked with a
  non-witness inventory type.
- `net` enforces the protocol limits (2000 headers, 50 000 inventory items, 101 locator hashes).
  A message over a limit is logged and dropped: it is a bug in the caller.

## `net.misbehaving`

One-way. Disconnect the peer and discourage its address for `discourage_hours`. Used when a peer sent something invalid
by consensus or broke the protocol in a way only the caller can see.

| Param | Type | Required | Description |
|---|---|---|---|
| `peer` | integer | yes | PeerId |
| `reason` | string | yes | Logged and included in `net.peer_disconnected` |

Manual peers are disconnected but not discouraged.

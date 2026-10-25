# net RPCs

Requests and responses are defined by the schemas in [schemas/rpc/](schemas/rpc/); shared types are in
[schemas/types.json](schemas/types.json) and [common.json](../../schemas/common.json).

| RPC | Operator | Schema |
|---|---|---|
| [`net.info`](#netinfo) | ✓ | [info.json](schemas/rpc/info.json) |
| [`net.peers`](#netpeers) | ✓ | [peers.json](schemas/rpc/peers.json) |
| [`net.connect`](#netconnect) | ✓ | [connect.json](schemas/rpc/connect.json) |
| [`net.disconnect`](#netdisconnect) | ✓ | [disconnect.json](schemas/rpc/disconnect.json) |
| [`net.send`](#netsend) | | [send.json](schemas/rpc/send.json) |
| [`net.misbehaving`](#netmisbehaving) | | [misbehaving.json](schemas/rpc/misbehaving.json) |

## `net.info`

Listening state, connection counts and traffic totals.

## `net.peers`

Connected peers. `ping_ms` is `null` until the first `pong`.

## `net.connect`

Open a manual connection. It is reconnected if lost, until the node restarts. Returns the new peer's id once the
handshake completes.

Errors: `invalid_params` (cannot parse or resolve), `rejected` with `data.reason` (`connection refused`, `timeout`,
`handshake failed`, `rejected by hook: <reason>`, `already connected`).

## `net.disconnect`

Close a connection. Returns `{}`. A manual peer disconnected this way is not reconnected.

Errors: `not_found`.

## `net.send`

Queue a message for a peer. Returns `{}` once the message is queued. Callers usually do not wait for the answer.
Messages from one caller to one peer are sent in the order the requests arrive.

The message is a `PeerMessage` ([types.json](schemas/types.json)): `getheaders`, `headers`, `inv`, `getdata` or
`block`, selected by its `type` field.

- For `block`, `witness: false` makes `net` serialize without witness data (BIP144), for peers that asked with a
  non-witness inventory type.
- The schema carries the protocol limits (2000 headers, 50 000 inventory items, 101 locator hashes). A message over a
  limit is a bug in the caller.

Errors: `not_found` if the peer is gone, which is normal: the caller may not have processed `net.peer_disconnected`
yet. `invalid_params` for a message that does not match the schema.

## `net.misbehaving`

Disconnect the peer and discourage its address for `discourage_hours`. Used when a peer sent something invalid
by consensus or broke the protocol in a way only the caller can see. Returns `{}`. Manual peers are disconnected but
not discouraged.

Errors: `not_found` if the peer is gone.

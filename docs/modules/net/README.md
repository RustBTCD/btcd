# net

`net` is the module responsible for handling peers, and the CRUD for peers. It maintains the `peer-list` table: all
known peers and the features stored for each of them. It knows nothing about messages except `ping`/`pong` and which
module must process each message.

## State

| State                                                                   | Where             |
|-------------------------------------------------------------------------|-------------------|
| Known peers: address, features                                          | table `peer-list` |
| Per known peer while the process runs: id, connection state, ping times | memory            |

For the `peer-list` table details see ([storage.md](../../architecture/storage.md#peer-list)). In memory, per known
peer:

| Field                    | Description                                                       |
|--------------------------|-------------------------------------------------------------------|
| `id`                     | Peer identifier assigned by `net`, unique while the process runs. |
| `state`                  | `known` (not connected), `connecting`, `connected`.               |
| `last_ping`, `last_pong` | Times of the last `ping` sent and the last `pong` received.       |

---

## Peers

Known peers come only from the configuration (`peers`). Configured peers are trusted. `net` keeps a connection to every
configured peer. When a peer disconnects, `net` connects to it again after `reconnect_delay_secs`. `net` does not accept
inbound connections.

---

## Connection flow

```
Connect TCP -> Assume peer connected -> Exchange features
```

TBD

---

## Pings

Every 30 seconds `net` sends a `ping` with a random 64-bit nonce to each connected peer (BIP31). `net` responds to every
`ping` with a `pong` carrying the same nonce. A `pong` whose nonce matches the last `ping` sets `last_pong`. If the peer
does not send the pong – disconnect and try connecting again.

---

## Message exchange

Each message type (command) is bound to exactly one module. A message of that type is only sent by that module and only
delivered to that module. For example, `getblocktxn` can only be sent or received by `block-sync`. In v0.1 `block-sync`
is the only module besides `net` that exchanges messages with peers ([routing table](#routing-table)).

`net` reads only the message header of the v1 transport: network magic, command, payload length, checksum. It decodes
payloads only for `ping`, `pong` and the [connection flow](#connection-flow). Every other payload is
passed to its owner as raw bytes.

A message is invalid when the magic is wrong, the checksum does not match, or the payload length exceeds 4 000 000
bytes (Core's `MAX_PROTOCOL_MESSAGE_LENGTH`).

### Requesting a peer

1. `block-sync` decides on the recipient peer.
2. `block-sync` forms a raw P2P message: command and encoded payload.
3. `block-sync` initiates the sending request to the peer (`send`).
4. `net` prepares the message (message header: magic, command, length, checksum) and sends it.

### Processing a request

1. `net` receives a message. It can also be a response to one of our previous requests.
2. `net` routes the message to the module that owns its command ([routing table](#routing-table)): it pushes
   `{peer, command, payload}` to the `block-sync` queue. If the queue is full, it is a node failure and the node stops.
3. `block-sync` initiates a request to respond to the peer, if a response is needed.

### Routing table

| Command                                                        | Owner        |
|----------------------------------------------------------------|--------------|
| `ping`, `pong`                                                 | `net`        |
| *Commands of the connection flow*                              | `net`        |                                               
| `headers`, `block`, `inv`, `notfound`, `getheaders`, `getdata` | `block-sync` |
| *Every other command*                                          | none in v0.1 |

---

## Interface

Direct calls. In v0.1 the only caller is `block-sync`.

### get_peers(state?)

Returns known peers: `peer-list` rows with their in-memory fields

### get_peer(id)

Return the peer from `peer-list` with its in-memory fields, or not found

### add_peer(...), remove_peer(id)

Add or remove peer to the `peer-list`.

### connect(id)

Starts the connection flow with peer (id)

### disconnect(id)

Loses the connection with peer (id); the state becomes `known`

### send(id, command, payload)

Send the message to the peer (id) if it is `connected`. The output will be routed to the queue.

---

## Configuration

```toml
[net]
peers = []                 # peers to connect to: ["host:port", "host:port"]
reconnect_delay_secs = 30  # delay before reconnecting to a disconnected peer
```

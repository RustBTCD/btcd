# Communication

This file is mostly refer to ([net](../modules/net/README.md)) module. It gives a high-level overview of how we
establish connections with peers and exchange messages.

## Connecting to the peer

```mermaid
sequenceDiagram
    autonumber
    participant N as net
    participant P as peer
    N ->> P: TCP connect
    Note over N: peer state = connected
    N -) P: features
    P -) N: features
    Note over N: store peer features in peer-list
```

### Features

TBD

---

## Pings

```mermaid
sequenceDiagram
    participant N as net
    participant P as peer
    loop every 30 s
        N -) P: ping (nonce)
        P -) N: pong (nonce)
    end
    P -) N: ping (nonce)
    N -) P: pong (nonce)
```

### Missing pong

If the pong is missing we disconnect the peer and initiate reconnect.

---

## Requesting a peer

```mermaid
sequenceDiagram
    participant S as block-sync
    participant N as net
    participant P as peer
    Note over S: 1. decide on the recipient peer
    Note over S: 2. form a raw P2P message<br/>(command, payload)
    S ->> N: 3. net.send(peer, command, payload)
    Note over N: 4. prepare the message
    N -) P: message
```

---

## Processing a request

```mermaid
sequenceDiagram
    autonumber
    participant P as peer
    participant N as net
    participant Q as block-sync queue
    participant S as block-sync
    P -) N: message
    Note over N: check the message header<br/>decide on message processor
    N -) Q: push {peer, command, payload} to corresponding queue
    Q -) S: consume {peer, command, payload}
    Note over S: parse payload, process, prepare response if needed
    S ->> N: net.send(peer, command, payload)
```

### Example

`block-sync` asks a peer for the headers that follow its tip at height 1000. The peer's chain extends ours.

```mermaid
sequenceDiagram
    autonumber
    participant S as block-sync
    participant Q as queue
    participant N as net
    participant P as peer
    Note over S: build the block locator<br/>heights 1000 … 990, 988, 984, … 480, 0
    S ->> N: net.send(peer, getheaders, payload)
    Note over N: add the message header<br/>magic, "getheaders", length, checksum
    N -) P: getheaders
    Note over P: first locator hash on its chain: height 1000<br/>take up to 2000 headers after it
    P -) N: headers (heights 1001 … 3000)
    Note over N: check the message header<br/>"headers" is owned by block-sync
    N -) Q: push {peer, headers, payload}
    Q -) S: consume {peer, headers, payload}
    Note over S: parse 2000 headers<br/>first header's parent is the block at 1000
```

> Note: P2P messages carry no request id. `block-sync` recognises the `headers` message as the answer to its
`getheaders` by the peer it comes from and by its content: the first header's parent is a block from the locator.

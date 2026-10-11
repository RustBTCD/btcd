# Peer lifecycle

Rules: [net](../daemons/net/README.md), [hook table](../architecture/events-and-hooks.md#hook-table), [block-sync](../daemons/block-sync/README.md#per-peer-state), [tx-relay](../daemons/tx-relay/README.md#peers).

## Outbound connection and handshake

```mermaid
sequenceDiagram
    autonumber
    participant P as peer P7
    participant N as net
    participant H as hook subscribers (none in v0.1)
    participant BS as block-sync
    participant TR as tx-relay

    Note over N: pick address from list<br/>(distinct network group)
    N->>P: TCP connect
    N->>P: version
    P->>N: version
    Note over N: checks: protocol version, services,<br/>self-connection nonce
    N->>H: hook.peer_connected
    H-->>N: continue
    P->>N: wtxidrelay, sendaddrv2
    N->>P: wtxidrelay, sendaddrv2, verack
    P->>N: verack
    N->>P: sendheaders, getaddr
    N-)BS: net.peer_connected (peer 7, services, start_height, …)
    N-)TR: net.peer_connected (relay, wtxid_relay)
    BS-)N: net.send getheaders (locator)
    TR-)N: net.send feefilter
```

- Step 4 runs through `rbtcd`, which calls subscribers one by one. With no subscribers, `rbtcd` answers `continue` immediately.
- If the hook rejects, `net` closes the connection without `verack` and emits no event.
- Step 12: `block-sync` sends `getheaders` to a new peer when it is the IBD sync peer or when the node is out of IBD ([block-sync](../daemons/block-sync/README.md#header-synchronisation)).
- Step 13: during IBD the `feefilter` value is the maximum, asking the peer not to announce transactions.

Inbound connections are the same with the first `version` coming from the peer.

## Messages after the handshake

Every message the peer sends is decoded by `net` and either handled inside `net` or emitted as an event for exactly one subscriber ([net message table](../daemons/net/README.md#inbound-message-handling)).

```mermaid
sequenceDiagram
    participant P as peer P7
    participant N as net
    participant BS as block-sync
    participant TR as tx-relay
    P->>N: ping
    N->>P: pong
    P->>N: inv (1 block, 3 wtx)
    N-)BS: net.inv_blocks (1 hash)
    N-)TR: net.inv_txs (3 items)
    P->>N: addrv2
    Note over N: stored, rate limited
```

## Misbehaviour

```mermaid
sequenceDiagram
    participant P as peer P7
    participant N as net
    participant BS as block-sync
    participant C as chain
    participant TR as tx-relay
    P->>N: headers (invalid proof of work)
    N-)BS: net.headers
    BS->>C: chain.submit_headers
    C-->>BS: error invalid, reason high-hash
    BS-)N: net.misbehaving (peer 7, high-hash)
    Note over N: disconnect, discourage address 24 h
    N-)BS: net.peer_disconnected (misbehaving: high-hash)
    N-)TR: net.peer_disconnected
    Note over BS: drop peer state,<br/>reschedule its blocks in flight
    Note over TR: drop peer state,<br/>move its requests to other announcers
```

Violations `net` detects itself (undecodable message, size limit, message before handshake, a request for a service that is not offered) follow the same path from "disconnect" on.

## Disconnection for other reasons

Timeouts (`net`'s keep-alive, `block-sync`'s download and stall timeouts through `net.disconnect`), the peer closing the connection, and the operator's `net.disconnect` end the same way: one `net.peer_disconnected` to each subscriber, after every other event about that peer. A `net.send` to a peer that is already gone is dropped silently.

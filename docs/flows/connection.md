# Connection

Module: [net](../modules/net/README.md).

## Connection flow

```
Connect TCP -> Assume peer connected -> Exchange features -> Exchange tips
```

```mermaid
sequenceDiagram
    autonumber
    participant N as net
    participant P as peer
    N->>P: TCP connect
    Note over N: peer state = connected
    N-)P: features
    P-)N: features
    Note over N: store peer features in peer-list
    N-)P: tip
    P-)N: tip
    Note over N: keep peer tip in memory
```

- Steps 2–3, exchange features: **[Q3](../open-questions.md#q3)**.
- Steps 4–5, exchange tips: **[Q4](../open-questions.md#q4)**.
- Which peers are connected and how many: **[Q5](../open-questions.md#q5)**.

## Pings

```mermaid
sequenceDiagram
    participant N as net
    participant P as peer
    loop every 30 s
        N-)P: ping (nonce)
        P-)N: pong (nonce)
    end
    P-)N: ping (nonce)
    N-)P: pong (nonce)
```

A missing `pong`: **[Q7](../open-questions.md#q7)**.

## Requesting a peer

```mermaid
sequenceDiagram
    participant S as block-sync
    participant N as net
    participant P as peer
    Note over S: 1. decide on the recipient peer
    Note over S: 2. form a raw P2P message<br/>(command, payload)
    S->>N: 3. send(peer, command, payload)
    Note over N: 4. prepare the message<br/>(header, encryption, auth)
    N-)P: message
```

Encryption and authentication: **[Q2](../open-questions.md#q2)**.

## Processing a request

```mermaid
sequenceDiagram
    autonumber
    participant P as peer
    participant N as net
    participant Q as block-sync queue
    participant S as block-sync
    P-)N: message (request, or response to our request)
    Note over N: check the message header<br/>find the owner of the command
    N-)Q: push {peer, command, payload}
    Q-)S: next item
    S->>N: send(peer, command, payload), if a response is needed
    N-)P: response
```

Routing: [net routing table](../modules/net/README.md#routing-table). Messages without an owner: **[Q9](../open-questions.md#q9)**. Invalid messages: **[Q26](../open-questions.md#q26)**.

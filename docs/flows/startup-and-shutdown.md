# Startup and shutdown

Rules: [lifecycle.md](../architecture/lifecycle.md), [manifest.md](../architecture/manifest.md), [storage.md](../architecture/storage.md#startup-reconciliation).

## Startup

```mermaid
sequenceDiagram
    autonumber
    participant M as rbtcd
    participant U as utxo
    participant N as net
    participant C as chain
    participant BS as block-sync
    participant TR as tx-relay

    Note over M: read config, lock datadir,<br/>open operator socket
    par spawn all
        M->>U: spawn (fd 3)
        M->>N: spawn
        M->>C: spawn
        M->>BS: spawn
        M->>TR: spawn
    end
    U->>M: bus.init (manifest)
    N->>M: bus.init
    C->>M: bus.init
    BS->>M: bus.init
    TR->>M: bus.init
    Note over M: validate registry,<br/>order: utxo, net, chain, block-sync, tx-relay

    M-->>U: init response (network, datadir, config)
    Note over U: open chainstate store
    U-)M: bus.ready
    M-->>N: init response
    Note over N: load addresses.json<br/>(no connections yet)
    N-)M: bus.ready
    M-->>C: init response
    Note over C: open index and blocks stores,<br/>load block index
    C->>U: utxo.info
    U-->>C: tip
    Note over C: rebuild active chain from tip
    C-)M: bus.ready
    M-->>BS: init response
    BS->>C: chain.info
    C-->>BS: tip, best header, ibd
    BS-)M: bus.ready
    M-->>TR: init response
    TR->>C: chain.info
    C-->>TR: ibd
    TR-)M: bus.ready

    M-)U: bus.start
    M-)N: bus.start
    M-)C: bus.start
    M-)BS: bus.start
    M-)TR: bus.start
    C-)N: chain.tip_changed (initial)
    C-)BS: chain.tip_changed (initial)
    C-)TR: chain.tip_changed (initial)
    Note over C: best-chain activation:<br/>connect stored blocks beyond the tip
    Note over N: listen, resolve seeds,<br/>open outbound connections
```

Notes:

- Steps 2–6: `rbtcd` answers no `bus.init` until all manifests are in and the registry is valid. A daemon that sends an invalid manifest makes the whole startup fail with the full list of problems.
- Each daemon initialises only after its dependencies are ready, so `chain` can call `utxo.info`, and `block-sync` and `tx-relay` can call `chain.info`, during initialisation.
- No event exists before `bus.start`. The initial `chain.tip_changed` gives every subscriber the starting tip.

## Recovery after a crash

Nothing special runs. The normal startup above is the recovery:

```mermaid
sequenceDiagram
    participant C as chain
    participant U as utxo
    Note over C,U: before the crash: chain stored blocks 101–105,<br/>utxo committed 101–103, then power loss lost 103
    C->>U: utxo.info
    U-->>C: tip = 102
    Note over C: active chain := genesis … 102<br/>best stored chain := … 105
    Note over C: after bus.start: activation
    C->>U: utxo.connect_block 103
    U-->>C: ok
    C->>U: utxo.connect_block 104
    U-->>C: ok
    C->>U: utxo.connect_block 105
    U-->>C: ok
```

## Shutdown

```mermaid
sequenceDiagram
    autonumber
    actor O as operator
    participant M as rbtcd
    participant TR as tx-relay
    participant BS as block-sync
    participant C as chain
    participant N as net
    participant U as utxo

    O->>M: node.stop
    M-->>O: {}
    Note over M: stop delivering events,<br/>refuse new operator requests
    M->>TR: bus.shutdown
    TR-->>M: done
    Note over TR: exit
    M->>BS: bus.shutdown
    BS-->>M: done
    M->>C: bus.shutdown
    Note over C: finish the current utxo call, if any
    C-->>M: done
    M->>N: bus.shutdown
    Note over N: close peer connections,<br/>save addresses.json
    N-->>M: done
    M->>U: bus.shutdown
    Note over U: flush chainstate
    U-->>M: done
    Note over M: remove socket, release lock, exit 0
```

## A daemon fails

```mermaid
sequenceDiagram
    participant M as rbtcd
    participant BS as block-sync
    participant Others as other daemons
    BS--xM: process exits (bus connection closed)
    Note over M: log failure and last stderr lines
    M->>Others: bus.shutdown (reverse order, skipping block-sync)
    Others-->>M: done
    Note over M: exit 1
```

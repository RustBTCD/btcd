# Bode: Initial Block Download

During IBD all other processes are paused, except maintaining the connections with peers.
Messages in the `block-sync` queue that are not part of the IBD are skipped. A message is part of the IBD when it
answers a request `block-sync` sent for the IBD.

The node enters IBD on first launch, when the database is clear and tip is genesis block.

## Downloading headers

1. `block-sync` selects one peer, using `net` as the CRUD for peers.
2. `block-sync` requests `getheaders` from it, in batches of 2000 headers. The first request carries a locator of the
   active chain; each next request carries the last header
   received ([block locator](../modules/block-sync/README.md#block-locator)).
3. `block-sync` continues until it has downloaded all headers this peer knows: an answer with fewer than 2000 headers.
4. `block-sync` validates the whole header chain by calling `chain.validate_headers`, and stores it with
   `chain.store_headers`.

```mermaid
sequenceDiagram
    autonumber
    participant BS as block-sync
    participant N as net
    participant P as sync peer
    participant C as chain
    BS ->> N: net.peers(status=connected)
    N -->> BS: connected peers
    Note over BS: select one peer
    loop until an answer has fewer than 2000 headers
        BS ->> N: net.send(peer, "getheaders", getheaders)
        N -) P: getheaders
        P -) N: headers
        N -) BS: queue: {peer, "headers", headers}
    end
    BS ->> C: chain.validate_headers(headers[..])
    C -->> BS: ok
    BS ->> C: chain.store_headers(headers[..])
    C -->> BS: ok
```

At this point the node has the ~full header history, validated on the header level only. The downloaded headers form the
candidate header chain.

---

## Downloading blocks

Blocks are downloaded from the beginning.

1. The current tip block is at height `tip`: all headers, and all blocks up to and including the tip block, are
   validated and stored.
2. `block-sync` knows the candidate header for `tip+1`. It sends `getdata` (`MSG_WITNESS_BLOCK`) for it to the sync
   peer; if the sync peer does not answer, to random eligible peers ([peer selection](../modules/block-sync/README.md#peer-selection)). Requests are made in batches (TBD).
3. When the data for block `tip+1` is received, `block-sync` requests validation and tip update from `chain` (
   `connect_block`).
4. If the block is valid and follows the tip block, `chain` updates the tip. Otherwise, the block is rejected with the
   corresponding error.
5. If the block is rejected:
    - if the rejection reason shows a mutated block, `block-sync`
      requests the block from another peer and returns to step 1;
    - otherwise the block is invalid by consensus while its header is valid: `block-sync` reports a consensus failure
      and the node stops.

```mermaid
sequenceDiagram
    autonumber
    participant BS as block-sync
    participant N as net
    participant P as sync peer, then random peers
    participant C as chain
    BS ->> C: chain.tip()
    C -->> BS: tip
    Note over BS: knows candidate headers tip+1..tip+n<br/>
    BS ->> N: net.send(sync peer, "getdata", getdata[tip+1..tip+n])
    N -) P: getdata
    P -) N: block[tip+1..tip+n]
    N -) BS: queue: {peer, "block", block[tip+1..tip+n]}
    Note over BS: if no answer within the request timeout, or notfound —<br/>request the missing blocks from a random connected peer,<br/>until collects all of them
    loop for k in 1..n
        BS ->> C: chain.connect_block(tip+k)
        alt Block valid and follows the tip
            Note over C: updates tip
            C -->> BS: ok
        else rejected
            C -->> BS: rejected {reason}
            Note over BS: step 5
        end
    end
```


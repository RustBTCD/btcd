# Open questions

Each question is referenced in the specification as **[Qn]**. "Current text" is what the specification states until the question is answered.

## Naming

### Q1

The requirement names the block module both `block-sync` and `block-syncer`.

Current text: `block-sync`.

## net

### Q2

Transport. Step 4 of "Requesting a peer" says `net` prepares the message with encryption and authentication. Is the BIP324 v2 encrypted transport in v0.1, or only the v1 plaintext transport?

Current text: the v1 message header (magic, command, length, checksum); encryption and authentication are unresolved.

### Q3

"Exchange features":

- Which messages: `version`/`verack` only, or also `wtxidrelay`, `sendaddrv2` (both only valid before `verack`), `sendheaders`, `sendcmpct`?
- Which services do we advertise, given that we serve no blocks or transactions?
- Which `relay` flag do we send in `version`, given that we do not accept transactions?
- `net` knows no messages except `ping`/`pong`. Which module handles the messages of the connection flow?

Current text: `net` handles the connection flow messages; their content is unresolved.

### Q4

"Exchange tips": which messages carry the tips (the `start_height` field of `version`, `getheaders`/`headers`, something else), and is a peer's tip updated after the connection flow? Block download uses the peer's tip to pick peers that have a block.

### Q5

Peers:

- Where known peers come from: configuration, DNS seeds, `addr`/`addrv2` messages.
- How many outbound connections are kept, and whether inbound connections are accepted.
- How a peer is chosen for a new connection.

Current text: `net.peers` and `net.max_outbound = 8` in the configuration.

### Q6

Is the peer's tip also stored in `net`'s `peer-list` table?

Current text: `peer-list` stores address and features; connection state, tips and ping times are kept in memory.

### Q7

What happens when a `pong` does not arrive: timeout value, disconnect, retry?

### Q8

`inv`, `getdata` and `notfound` carry items for both blocks and transactions, but a message type is bound to one module. Is the whole message routed to `block-sync`, or does `net` split items by type (which requires `net` to decode the payload)?

Current text: `inv` and `notfound` are routed to `block-sync` whole.

### Q9

Messages without an owner in v0.1 (`tx`, `addr`, `addrv2`, `getaddr`, `mempool`, `feefilter`, `sendcmpct`, …, and `getheaders`/`getdata` received from peers):

- Are they dropped silently, or does `net` disconnect the peer?
- Are `getheaders`/`getdata` from peers answered (serving headers and blocks), given that blocks are not relayed?

Current text: no owner.

### Q10

Does `net` notify `block-sync` when a peer connects or disconnects, or does `block-sync` only read peers through `net.peers`?

Current text: no notifications.

## block-sync and chain

### Q11

Header download step 3 validates the whole header chain after all headers of the peer have been downloaded. Is validation done once at the end, or per batch of 2000 as batches arrive? The choice changes when a bad header is detected and how many headers are held in memory (about 900 000 on mainnet).

Current text: once at the end.

### Q12

Sync peer:

- How the single peer for header download is selected (random, highest tip from "exchange tips", …).
- What happens if it disconnects, stalls, or sends invalid headers.
- How a sync peer that serves a valid but low-work header chain is detected.

### Q13

Batched block download: batch size, number of requests in flight, how many received blocks may wait for their predecessor.

Current text: `blocks_per_request = 16`, `max_requests_in_flight = 8`.

### Q14

A rejected block: "return to step 1" re-requests the same block at the height after the tip block.

- If the block is invalid by consensus, and not only mutated by the peer that sent it, every peer serves the same block. Are its header and the headers after it marked `invalid`, and how is a different header chain obtained?
- Is the peer that sent it penalised?

### Q15

Block validation:

- Script verification engine (libbitcoinkernel, or native Rust).
- Whether assumevalid is used.

Current text: Bitcoin Core's checks; the script engine is unresolved.


### Q16

How peers announce new blocks in standard mode. Peers announce with `headers` only after we send `sendheaders` (BIP130); otherwise with `inv`. Do we send `sendheaders`? What does `block-sync` do with an `inv` for a block?

### Q17

Standard mode step 2 requests the block from a random peer. A random peer may not have the block yet. Is the announcing peer used, and what happens if the chosen peer does not have it (`notfound`, timeout)?

### Q18

"All other processes are paused" during IBD and reorg:

- Which processes, in v0.1, besides maintaining connections?
- What happens to messages routed to `block-sync` that are not part of the running IBD or reorg (for example a new header announcement): dropped, or deferred until the mode ends?

### Q19

The last common ancestor search runs on random peers. A random peer may not have switched to the new branch, and then the search finds the ancestor with that peer's chain instead. Is the search run on the peer that sent the new header?

### Q20

Reorg step 2, "valid chain work": which condition starts a reorg? The cumulative chain work of the new branch compared with the tip's, or the height (the new header at the height after the tip block)? Around difficulty adjustments, a branch with more blocks can have less work.

### Q21

Failure during a reorg:

- If a block of the new branch is rejected after the tip was downgraded to the last common ancestor, is the old branch reconnected from its stored blocks?
- After a crash during steps 6–7, the tip is somewhere between the last common ancestor and the new branch. How does `block-sync` continue after restart?

### Q22

"Current header or older" is rejected. Is the rejected header stored? A header at the tip block's height or lower can start a branch that later becomes longer.

## Node

### Q23

IBD entry and exit:

- Is the node always in IBD at startup?
- When does it switch from IBD to standard mode (all headers of the sync peer connected, tip age, something else)?
- How is `block-sync`'s candidate header chain rebuilt after a restart, when headers above the tip are already stored?

### Q24

Operator interface: is there an RPC or CLI in v0.1 to read the tip, a block, a header, and peers?

Current text: none.

### Q25

Concurrency and queues:

- Threads, async runtime, or both, and which module runs where.
- Capacity of the `block-sync` queue and what `net` does when it is full (stop reading from peers, drop messages).

### Q26

Misbehaving peers (invalid headers, invalid blocks, invalid message headers): disconnect, ban, nothing?

### Q27

Timeouts for `getheaders` and `getdata` requests, and what happens on timeout.

Current text: `request_timeout_secs = 120`; behaviour on timeout unresolved.

### Q28

Shutdown: signals, order of stopping modules, flushing stores.

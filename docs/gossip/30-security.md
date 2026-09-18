# 30. Threat model and rationale

Almost every rule in this specification exists because of an attack. This module states the adversary model, describes the attacks, and gives the rationale for each rule whose purpose is not evident from its text. It is informative: the rules themselves are in the normative modules.

## 1. Adversary model

Assume an adversary that:

- Can open connections to any listening node, in any number, subject only to its own resources.
- Can operate listening nodes and cause their addresses to be gossiped.
- Can name arbitrary addresses in gossip, whether or not anything listens there.
- Can observe, delay, drop and reorder traffic on paths it controls.
- Cannot break the cryptographic primitives, and cannot find proof of work faster than its share of hash rate allows.

Assume further that **nothing a peer asserts is verifiable**. Service flags, timestamps, chain heights, user agents and address records are unauthenticated claims. Only proof of work and the internal consistency of a block or transaction are self-validating.

The network has no identity layer. A node cannot distinguish one adversary holding a hundred connections from a hundred independent peers, except by the cost of the resources each connection demonstrably consumes. Every defence here is a device for making an attack cost real resources rather than repeated messages.

## 2. Eclipse

An eclipse succeeds when every connection a node holds terminates at the adversary. The victim sees only the chain and transactions the adversary shows it.

**Inbound** connections are cheap: the adversary dials. The defences are to bound them and never let them displace outbound connections (`SEL-17`, `SEL-18`), and to make eviction select against the adversary by protecting peers whose value cannot be counterfeited without real resources (`SEL-19`).

**Outbound** connections are chosen by the victim from its address store, so the adversary must control what the store contains and which entry is chosen:

- `ADR-25`, `BST-07`: bound any single source's share of the store, so flooding from one peer cannot dominate it.
- `ADR-06`: bound the rate at which records are accepted at all, so flooding is slow as well as bounded. Shuffling before discarding the excess denies the sender the choice of which records survive.
- `ADR-08`: the limit is silent, so a sender that ignores it loses records without knowing.
- `SEL-09`: prefer addresses the node has connected to over addresses it was told about. Naming an address is free; having one promoted requires operating a reachable node. This is the central defence.
- `SEL-10`, `SEL-11`: verify reachability on the node's own schedule, and require that an incumbent be shown unreachable before it is displaced. Without the latter, one reachable adversary node can displace every verified address by repetition.
- `SEL-04`, `SEL-01`: require net-group diversity, so the adversary must control many distinct network resources. Privacy-network addresses are exempt because their groups bear no relation to cost.
- `SEL-13`: a node with one outbound connection is trivially eclipsed. Eight full-relay plus two block-relay-only is the network convention; substantially fewer is easier to partition, substantially more consumes capacity other nodes need.
- `SEL-15`: restart is the moment of maximum exposure, because every connection is redrawn from a store the adversary may have spent time populating. Keeping a few prior block-relay-only peers, and discarding the list after use, denies a forced restart any repeatable effect.
- `BST-02`, `BST-01`: first start is the one moment a single party can select every peer. A seed operator, a compromised resolver or a stale fixed list can all supply adversary-controlled addresses.
- `ENC-18`, `ENC-14`: unroutable, zero-port, unreachable and retired-network addresses can never yield a connection; storing them wastes capacity and biases selection.
- `ADR-22`: the timestamp orders selection and governs relay eligibility. A peer that sets it arbitrarily would otherwise control both.
- `CON-49`: the address store and the outbound slots exist to find peers that serve the chain. A peer that cannot serve blocks occupies a slot without providing what the slot is for.

## 3. Topology inference

An adversary that maps which nodes are connected to which can target the links that matter.

- **Relay timing.** Relaying a record immediately reveals the link it came from. `ADR-34` batches on a randomised schedule.
- **Reflection.** Sending a record back toward its source confirms the link (`ADR-15`, `ADR-24`), and confirms which records the store accepted, an oracle for probing it.
- **Solicitation probing.** An adversary injects a distinctive address and later asks for it back. `ADR-30` denies the probe on outbound connections, because a node that answers on connections it dialled can be probed by any peer it chose; it limits answering to once per connection; `ADR-32` and `ADR-33` make the answer small, random, stale and identical across requesters, so repeated sampling yields nothing new. A fresh timestamp in a response would otherwise reveal a recent connection.
- **Cross-network correlation.** A node reachable on two networks can be identified as one node by comparing its responses. `ADR-33` derives responses separately per network; `ADR-13` keeps self-announcements on the network they belong to.
- **Amplification.** `ADR-16` and `ADR-18`: relaying to more than two peers multiplies traffic without improving propagation; choosing the two afresh each time lets a peer that repeats a record reach a growing fraction of the node's peers; predictable destinations let an adversary map the peer set. Relay only of small unsolicited messages, and not while awaiting a `getaddr` response, prevents bulk solicitation responses from being re-broadcast by every node that received them.
- **Connection-type disclosure.** Block-relay-only connections are useful precisely because they are not identifiable from traffic. `ADR-36` and `TXR-16` keep address and fee traffic off them.
- **Self-announcement.** `ADR-14`: a node that does not listen provokes failed connection attempts and reveals its location for nothing; an uncorroborated address, taken from a single peer's `addr_recv`, lets that peer make the node advertise an address it does not control.

## 4. Transaction origin

An adversary connected to many nodes and recording arrival times can often identify the node that first announced a transaction.

- `TXR-06`: immediate announcement discloses when the node first saw each transaction.
- `TXR-07`: an adversary holding several connections to a node, receiving independently randomised announcements, can average away the randomisation. One shared schedule across accepted connections denies that. Connections the node initiated are not chosen by the adversary and can use a shorter interval.
- `TXR-27`: serving any transaction on demand makes the node an oracle for the contents of its memory pool, and thereby for which transactions it saw first. `TXR-36` is the narrow exception: ancestor resolution needs a transaction never announced by that identifier.
- `TXR-19`: preferring outbound peers as sources means an adversary must expend real connection resources, not merely announce first, to control where a transaction is fetched from. Delaying txid announcements while wtxid peers exist gives an unambiguous announcement a chance to arrive. Delaying overloaded peers prevents one peer from absorbing the node's request capacity by announcing heavily.
- `TXR-01`: without wtxid relay, an adversary can announce a txid, supply a variant with altered witness data, and thereby block the node from obtaining the real transaction from any other peer for the duration of the request.

The deployed defence for a node's own transactions is the private broadcast role ([04-connection.md](04-connection.md) section 6): short-lived connections over a privacy network that deliver the transaction and close.

## 5. Fingerprinting

Distinguishing implementations lets an adversary target known defects and track a node across address changes. Contributing channels: the user agent (`CON-21`); serving side-chain blocks most nodes have discarded (`BLK-36`); on the encrypted transport, a fixed garbage length or long-form message types where short ones exist. Fingerprinting is a prerequisite for targeting, not an attack in itself.

## 6. Resource exhaustion

Every message a peer can cause a node to process is a cost imposed for free. Three structural rules:

- Never allocate on the basis of an unvalidated length (`ENC-06`, `T1-08`, `T1-07`).
- Bound per-peer state a peer can cause to be created, and discard rather than terminate when the bound is reached (`TXR-21`, `ADR-06`).
- Require proof of work before committing storage to a header chain (`BLK-11`), since headers are otherwise unlimited and cheap. `BLK-19` and `BLK-29` apply the same principle to block data: no download or validation on an announcement alone.

Transaction relay is the hardest case, because transactions carry no proof of work and validity is chain-dependent. `TXR-30` forbids punishing a peer for an invalid transaction, so the only defences are the caps and delays of [22-transaction-relay.md](22-transaction-relay.md) section 4.

Flow control: `T1-16` exists because peers bound the unsent data they hold per connection and stop reading that connection's input when the bound is reached; two nodes that each stop reading while waiting to write deadlock until the inactivity deadline.

## 7. Partitioning

An adversary need not eclipse a node completely. Holding some of its outbound connections and starving them of blocks delays its view of the chain.

- `SEL-22`: an outbound peer that fails to keep up may be on another chain, withholding blocks, or partitioned itself. It occupies one of the few connections that determine which chain the node follows.
- `SEL-23`: without protection, a node under attack can be induced to disconnect the honest peers keeping it on the correct chain.
- `SEL-24`: rotation against the peer that has gone longest without announcing new work makes silence an eviction criterion. Counting only real announcements of new work prevents a peer from retaining its slot by re-announcing known blocks. The grace period for new connections prevents the node from repeatedly evicting peers it just connected to.
- `BLK-12`: peers disagree during reorganisations and while synchronising; terminating on disagreement would partition the network along it. The exception for a low-work outbound peer during initial synchronisation exists because such a peer cannot help the node reach the chain at all.
- Block-relay-only connections (`SEL-13`) exist for this threat: an adversary that has captured every connection it can identify still has to find the ones it cannot.

## 8. Punishment as an attack surface

Where a protocol punishes misbehaviour, the punishment is itself attackable: an adversary that can make honest peers appear to misbehave can make them disconnect one another.

Punishment is therefore confined to violations that are unambiguous and locally verifiable: sizes over a fixed limit, structurally invalid messages, invalid proof of work, and data contradicting a commitment already received. Validity disagreements are never punished (`TXR-30`, `BLK-12`, `TXR-15`); compact block reconstruction failures are never attributed to the sender (`CMP-26`), because `CMP-25` permits relay before validation and reconstruction mixes in transactions the sender never sent; records are bounded in time and never permanent (`MSG-03`); loopback and shared privacy-network addresses are never recorded (`MSG-02`), because the record would punish unrelated peers; manual peers are never punished, because they express operator intent that outranks automatic policy and are the operator's remedy when automatic selection fails (`CON-44`).

`CMP-07`: a fixed short-identifier nonce would let an adversary construct a transaction colliding with one in a future block, failing reconstruction at every node.

## 9. Liveness and handshake

- `CON-41`, `CON-39`: connections to unresponsive peers consume slots a reachable peer would occupy, which is the resource an eclipse contends for.
- `CON-36`: pings are how dead connections are detected and how latency is measured; a node that never pings cannot be measured and may be evicted in preference to one that can.
- `CON-18`, `CON-19`, `CON-20`: the nonce is the only self-connection detection. Echoing a received nonce makes a peer believe it connected to itself. An undetected self-connection wastes a slot and may propagate the node's own address as a reachable peer.
- `CON-25`, `CON-26`: a peer's clock and height are trivially manipulated by an attacker holding several connections. Heights are not comparable across chains.
- `CON-45`: reachability probes and address solicitation are how nodes verify addresses without consuming long-lived slots. Treating them as failures would discard the addresses of nodes that are probing.
- `T2-01`: the encrypted transport authenticates nothing; treating a session as identity evidence would let a machine-in-the-middle inherit whatever trust the identity carried.
- `T2-26`, `T2-27`: an initiator retries with version 1 only if it received nothing. Any byte sent, including a well-formed `version` message, suppresses the retry, and the responder becomes unreachable to that initiator. Counting the failed attempt against the address would eventually mark a reachable peer unreachable.
- `T2-09`: a key matching the version 1 prefix would be classified as a peer on a different network.

## 10. Out of scope

This specification does not defend against:

- An adversary with a majority of hash rate.
- Traffic analysis by an observer who sees a node's whole link. The encrypted transport hides content, not the existence, size or timing of messages.
- Deanonymisation of an operator by means outside the protocol.
- A machine-in-the-middle on an unencrypted connection, and, absent out-of-band comparison of session identifiers, on an encrypted one.
- An adversary that controls the node's name resolution or bootstrap configuration.

# 11. Peer management

Which peers a node connects to, how it verifies that an address is usable, and which connections it drops when capacity is exhausted. How a node stores and indexes addresses is not specified; the properties the store must have are.

## 1. Net groups

A **net group** is an equivalence class over addresses. It approximates the cost of acquiring addresses: many addresses within one group are assumed cheap, addresses across many groups expensive.

| ID | Rule | Reaction |
| --- | --- | --- |
| `SEL-01` | MUST define a net group function over IPv4 and IPv6 addresses such that the number of groups an adversary can occupy is bounded by the network resources it controls, not by the addresses it can name. Addresses on networks derived from public keys (Tor, I2P, CJDNS) each form their own group and are exempt from `SEL-04`. | safety |

*Non-normative construction.* Group IPv4 by /16 and IPv6 by /32. Where a mapping to autonomous systems is available, group by autonomous system instead.

## 2. Connecting

| ID | Rule | Reaction |
| --- | --- | --- |
| `SEL-03` | SHOULD NOT initiate an automatic connection to a port in [90-parameters.md](90-parameters.md) section 4. Operators of those hosts see the attempt as scanning. | — |
| `SEL-04` | MUST NOT hold more than one automatic outbound IPv4 or IPv6 connection to the same net group at a time. Manual connections count toward occupied groups; reachability probes do not. | safety |
| `SEL-13` | MUST maintain at least two automatic outbound connections, and SHOULD maintain eight in the full-relay role plus two in the block-relay-only role. | safety |
| `SEL-15` | SHOULD, on restart, reconnect to a small number of the block-relay-only peers it held at its last clean shutdown, discarding that list once used. | — |

## 3. Verifying reachability

An address record asserts that a node is listening. Only a connection establishes that it is.

| ID | Rule | Reaction |
| --- | --- | --- |
| `SEL-09` | MUST distinguish addresses it has successfully connected to from addresses it was only told about, and MUST prefer the former when choosing an automatic outbound connection. | safety |
| `SEL-10` | MUST periodically open connections for the sole purpose of verifying that an address is reachable, terminating them once the handshake completes. | safety |
| `SEL-11` | Before displacing an address it believes reachable with a newly verified one, MUST verify that the existing address is no longer reachable. | safety |

## 4. Accepting inbound connections

Applies to `[LSN]`.

| ID | Rule | Reaction |
| --- | --- | --- |
| `SEL-17` | MUST bound the number of inbound connections it accepts, and MUST reserve capacity so that inbound connections never displace outbound ones. It MAY bound separately the number of inbound connections that relay transactions. | safety |
| `SEL-18` | When at capacity, MUST either refuse the new connection or terminate an existing inbound connection, and MUST NOT terminate an outbound connection to make room. | safety |
| `SEL-19` | When choosing an inbound connection to terminate, MUST protect peers whose demonstrated value an adversary cannot cheaply counterfeit, and MUST choose from the rest a peer in the most heavily represented net group, preferring one whose address it has recorded. | safety |
| `SEL-21` | MUST NOT evict a connection its operator configured. | safety |

RECOMMENDED protection criteria, each chosen because counterfeiting it costs real resources:

| Criterion | Why it resists counterfeiting |
| --- | --- |
| Lowest observed round-trip latency | Requires physical proximity. |
| Most recently delivered a block this node had not seen | Requires winning a race against the network. |
| Most recently delivered a transaction this node accepted | Requires the same for transactions. |
| Longest established connection | Requires having been present before the attack began. |
| Membership of an under-represented network | Requires operating on that network. |
| Diversity of net group | Requires controlling distinct network resources. |

## 5. Replacing unproductive outbound peers

An outbound peer that does not keep up with the chain provides no protection against partitioning.

| ID | Rule | Reaction |
| --- | --- | --- |
| `SEL-22` | MUST terminate an outbound connection whose peer has failed for about 20 minutes to demonstrate a chain with at least as much work as this node's own, after first requesting headers and allowing about two minutes for a response. | safety |
| `SEL-23` | MUST protect a bounded number of outbound peers from `SEL-22` once they have demonstrated a chain with at least as much work as its own. | safety |
| `SEL-24` | When its tip has not advanced for an extended period, MUST open an additional outbound connection and then terminate whichever outbound peer least recently informed it of a new block. Only an announcement by `headers` or `cmpctblock` of a block with more work than the node's tip counts; an announcement by `inv`, or of a block already known, does not. A peer connected for less than a short grace period is not a candidate. | safety |

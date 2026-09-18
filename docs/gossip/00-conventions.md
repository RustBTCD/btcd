# 00. Conventions

## 1. Normative language

The key words MUST, MUST NOT, SHOULD, SHOULD NOT and MAY are to be interpreted as described in RFC 2119 and RFC 8174. They have that meaning only in upper case.

This specification uses the levels as follows.

- **MUST, MUST NOT.** A requirement. There are two kinds, distinguished by the *Reaction* column of the rule tables:
  - **wire**: a conforming peer reacts to a violation, using one of the reactions in section 3. The reaction is what a peer does, and it is how an implementer recognises the failure.
  - **safety**: no peer can observe a violation, but a node that violates the rule is exposed to an attack or a resource exhaustion described in [30-security.md](30-security.md). These rules are what keep a node connected to the honest network under attack, and they are not optional.
- **SHOULD, SHOULD NOT.** A recommendation. Deviation is safe if the implementer accepts the cost, which [30-security.md](30-security.md) states. Used sparingly.
- **MAY.** Explicitly permitted behaviour. A node MUST tolerate it in a peer.

A rule without a capability tag binds every node. A rule tagged with a capability binds only nodes offering it. See section 5.

## 2. Rule format

Rules are rows in tables of the form:

| ID | Rule | Reaction |
| --- | --- | --- |
| `ENC-26` | MUST NOT include more than 101 hashes in a locator. | terminate |

The subject of every rule is the node being specified, unless the rule names another. The *Reaction* is one of the terms in section 3, or **safety** for a requirement that no peer can observe, or **—** for a SHOULD or MAY.

Text between tables is informative, except where it uses a key word in upper case.

## 3. Peer reactions

| Term | Meaning |
| --- | --- |
| **terminate** | The peer closes the connection. It MAY reconnect later. |
| **terminate and record** | The peer closes the connection and records the violating address, so that for a bounded period it will not initiate connections to that address, will not relay it, and will refuse inbound connections from it when inbound capacity is nearly exhausted. |
| **discard** | The peer drops the message and continues. |
| **ignore** | The peer parses the message, takes no action, and continues. No error condition arises. |
| **deprioritise** | The peer continues, but the violating node becomes a preferred candidate for eviction, or its effort is wasted. |

| ID | Rule | Reaction |
| --- | --- | --- |
| `MSG-01` | MUST implement *terminate*, or a stricter reaction, for every wire violation this specification attributes to it. | safety |
| `MSG-02` | MUST NOT terminate or record a peer its operator configured manually for a protocol violation. MUST NOT record a peer on a loopback address or one reached over a privacy network where the address does not identify a single host; it MAY still terminate it. | safety |
| `MSG-03` | MUST NOT make a recorded violation permanent without operator action. | safety |

## 4. Terminology

**Node.** A participant in the peer-to-peer network that implements this specification.

**Peer.** A node to which a connection exists. Nothing a peer asserts about itself is verifiable except by independent observation.

**Connection.** A bidirectional, ordered, reliable byte stream carrying framed messages.

**Inbound, outbound.** A connection the remote peer initiated; a connection this node initiated. **Initiator, responder.** The node that opened a connection; the node that accepted it. The distinction is security relevant throughout: an attacker chooses when to open an inbound connection but cannot choose to receive an outbound one.

**Automatic connection.** An outbound connection chosen by the node from its address store, as opposed to one configured by the operator.

**Address record.** A network address, a port, a set of service flags and a timestamp ([01-encoding.md](01-encoding.md) section 8).

**Announcement.** A message informing a peer that an object exists, by hash or by address, without transmitting it.

**Relay.** Forwarding an announcement or object received from one peer to others.

**Inventory.** A hash-identified object that can be announced and requested: a transaction or a block.

**Net group.** An equivalence class of addresses approximating the cost of acquiring addresses within it ([11-peer-management.md](11-peer-management.md) section 1).

**Common version.** The lesser of the two `version` values exchanged on a connection. Every version-gated rule tests the common version.

**Best chain, tip, chain work.** The valid chain with the greatest cumulative proof of work known to a node, its highest block, and that cumulative work. Chain work is comparable across nodes; height is not.

## 5. Capabilities and profiles

Conformance is defined against capabilities. Each capability adds rules; [91-conformance.md](91-conformance.md) indexes them.

| Tag | Capability | Advertised by |
| --- | --- | --- |
| *(untagged)* | Base. Connects out, negotiates, stays alive, synchronises headers and blocks. | — |
| `[LSN]` | Listening. Accepts inbound connections. | — |
| `[ADR]` | Address relay. | — |
| `[BSA]` | Archival block serving. Serves every block of the best chain. | `NODE_NETWORK` |
| `[BSL]` | Limited block serving. Serves at least the most recent 288 blocks. | `NODE_NETWORK_LIMITED` |
| `[TXR]` | Transaction relay. | — |
| `[CMP]` | Compact blocks. | — |
| `[ENC]` | Encrypted transport. | `NODE_P2P_V2` |
| `[PRV]` | Operates over a privacy network such as Tor or I2P. | — |

`[BSA]` and `[BSL]` both require `NODE_WITNESS` and witness serialisation. A node MAY advertise both `NODE_NETWORK` and `NODE_NETWORK_LIMITED`.

| Profile | Capabilities | Notes |
| --- | --- | --- |
| Outbound client | Base | Serves nothing, listens for nothing. The minimum useful node. |
| Full node | Base, `[LSN]`, `[ADR]`, `[BSA]`, `[TXR]`, `[CMP]` | The prevailing profile. `[ENC]` RECOMMENDED. |
| Pruned node | As full node, with `[BSL]` in place of `[BSA]` | |

A `[LSN]` node that offers neither `[BSA]` nor `[BSL]` is reachable only by manual connections: automatic peers terminate immediately after the handshake (`CON-49`), and address stores do not select it.

| ID | Rule | Reaction |
| --- | --- | --- |
| `CON-01` | MUST NOT advertise a service flag or capability it does not provide. | terminate (peers act on the advertisement and terminate the node for stalling once it fails to deliver) |

## 6. Notation

Field tables list, in wire order: name, size in bytes, type, description. `var` means variable length, encoded as defined in [01-encoding.md](01-encoding.md).

Byte strings are hexadecimal in transmission order, for example `f9 be b4 d9`. A hash is the 32-byte internal representation, which is the reverse of the display form.

## 7. Timers

A duration is specified only where a peer's behaviour depends on it. [90-parameters.md](90-parameters.md) collects three kinds:

1. **Deadlines a peer enforces.** A node MUST meet them.
2. **Delays a peer must tolerate.** A node MUST NOT treat them as failures.
3. **Rates a peer enforces.** Excess is discarded or the connection terminated.

Durations that only govern internal scheduling are deliberately absent, even where a widely deployed implementation has a well-known value. Where randomised timing is required, the rule that needs it says so.

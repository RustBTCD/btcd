# 04. Connection lifecycle

How a connection is established, what is negotiated and when, how liveness is maintained, how a connection ends, and which roles a connection can play. Message layouts are in [05-messages.md](05-messages.md).

## 1. States

| State | Entered when | Permitted |
| --- | --- | --- |
| `TRANSPORT` | Stream open. Absent on version 1. | Transport handshake only. |
| `AWAITING_VERSION` | Framing established. | `version`, once per direction. |
| `NEGOTIATING` | Peer's `version` received. | Negotiation messages (section 3), `sendheaders`, `sendcmpct`, `verack`. |
| `ESTABLISHED` | Peer's `verack` received. | All messages. |
| `CLOSING` | Termination decided. | None. |

The two directions progress independently: a node may have sent `verack` while still awaiting the peer's.

## 2. Handshake

```
  initiator                                responder
  version               ---------------->
                        <----------------  version
                        <----------------  [negotiation messages]
                        <----------------  verack
  [negotiation messages] --------------->
  verack                ---------------->
```

| ID | Rule | Reaction |
| --- | --- | --- |
| `CON-10` | As initiator, MUST send `version` first, without waiting for anything from the responder. | terminate (handshake deadline) |
| `CON-11` | As responder, MUST NOT send `version` before receiving the initiator's. | terminate (breaks transport detection, `T2-08`) |
| `CON-12` | MUST ignore any non-transport message received before the peer's `version`. | — |
| `CON-48` | Between the peer's `version` and its `verack`, MUST ignore any message other than `verack`, the negotiation messages of section 3, `sendheaders` and `sendcmpct`. | — |
| `CON-13` | MUST ignore a second `version` from a peer. | — |
| `CON-14` | MUST ignore a redundant `verack`. | — |
| `CON-40` | MUST complete the handshake, including any transport handshake, within 60 seconds of the stream being established. | terminate |

### The `version` message

Layout in [05-messages.md](05-messages.md) section 2.1. Fields from `nonce` onwards are historically optional; a node MUST accept a message truncated after any of them and apply the defaults `nonce` 1, `user_agent` empty, `start_height` −1, `relay` true.

| ID | Rule | Reaction |
| --- | --- | --- |
| `CON-15` | MUST send every field through `relay`. | — (defaults applied) |
| `CON-16` | MUST set `version` to at least 31800. | terminate |
| `CON-17` | SHOULD set `version` to 70017. Below 70016 a peer negotiates neither `addrv2` nor wtxid relay; 70016 forgoes `feature` negotiation. | — |
| `CON-34` | MUST NOT advertise a version above 70017. | — |
| `CON-18` | MUST generate `nonce` from a cryptographically strong source, afresh for every connection. | safety |
| `CON-19` | MUST NOT place in `nonce` a value received from any peer. | terminate (the peer concludes it connected to itself) |
| `CON-20` | MUST retain the nonces of its own outstanding outbound `version` messages and terminate an inbound connection whose `version` carries one of them. | safety |
| `CON-21` | MUST limit `user_agent` to 256 bytes. It SHOULD follow BIP 14, `/name:version/`, and SHOULD NOT identify the operator. | discard (the whole `version`; the connection then fails at the handshake deadline, presenting as an unexplained timeout) |
| `CON-23` | SHOULD set `addr_recv` to the address and port at which it observes the peer. A node uses this field, as reported by peers it did not dial, to learn its own external address, and MUST NOT treat any single report as authoritative. | — |
| `CON-24` | MUST set `addr_from` to zeros and MUST ignore it on receipt. | — |
| `CON-25` | MUST NOT rely on a peer's `timestamp` for any decision. It MAY aggregate several peers' values to warn its operator of clock skew. | safety |
| `CON-26` | MUST NOT rely on `start_height`. Chain comparisons MUST use cumulative work. | safety |
| `CON-49` | MUST terminate an automatic outbound connection whose peer's `version` does not advertise `NODE_WITNESS` together with `NODE_NETWORK`, or together with `NODE_NETWORK_LIMITED` when this node's tip is within 144 blocks of its best known header. Manual connections and reachability probes are exempt. | safety (consequence for listeners: a node not advertising these flags keeps no automatic inbound connection and is not selected from address stores) |

## 3. Negotiation

Negotiation messages are sent after this node's `version` and before its `verack`.

| Message | Sent when | Effect |
| --- | --- | --- |
| `wtxidrelay` | Common version ≥ 70016 | Transactions announced by wtxid ([22-transaction-relay.md](22-transaction-relay.md)). |
| `sendaddrv2` | Common version ≥ 70016 | Sender accepts `addrv2` ([10-address-gossip.md](10-address-gossip.md)). |
| `feature` | Common version ≥ 70017 | Advertises one named feature (BIP 434). Zero or more per connection. |
| `sendtxrcncl` | Only by a node implementing BIP 330 | Transaction reconciliation. Not further specified here. |

| ID | Rule | Reaction |
| --- | --- | --- |
| `CON-28` | MUST send every negotiation message after its own `version` and before its own `verack`. | terminate (when received after `verack`) |
| `CON-29` | MUST tolerate a peer that sends no negotiation message, falling back to the behaviour defined for the absent capability. | — |
| `CON-30` | MUST ignore a duplicate `wtxidrelay` or `sendaddrv2`. | — |
| `CON-32` | MUST NOT send a message gated on a version above the common version. | terminate |
| `CON-50` | MUST NOT send `feature` when the common version is below 70017. MUST ignore a `feature` whose identifier it does not recognise. | terminate (if sent below 70017, or if `feature_id` is shorter than 4 bytes, or if bytes follow `feature_data`) |
| `CON-31` | MUST accept `sendheaders` and `sendcmpct` at any point after the peer's `version`, before or after `verack`. | — |

Unknown message types received during negotiation are ignored under `MSG-10`.

## 4. Liveness

`ping` carries a `uint64` nonce; `pong` echoes it. For a common version of 60000 or below, `ping` has no payload and no `pong` is expected. No node at the minimum version is in that range, but a node MUST NOT fail on an empty `ping`.

| ID | Rule | Reaction |
| --- | --- | --- |
| `CON-35` | MUST answer every `ping` with a `pong` carrying the identical nonce. | terminate (ping deadline) |
| `CON-36` | SHOULD send a `ping` every two minutes, and SHOULD NOT send another while one is outstanding. Peers use the observed round trip when choosing whom to evict. | — |
| `CON-37` | MUST transmit at least one message on every connection within any 20-minute period. Answering the peer's pings satisfies this. | terminate |
| `CON-38` | MUST NOT terminate because a `pong` nonce does not match or arrives without an outstanding `ping`. A non-matching `pong` does not satisfy the outstanding request. | — |
| `CON-39` | MUST terminate a connection on which a `pong` for an outstanding `ping` has not arrived within 20 minutes. | — |
| `CON-41` | MUST terminate a connection on which no byte has been both sent and received within 60 seconds of establishment, and one on which no byte has been received for 20 minutes. | — |

## 5. Termination

| ID | Rule | Reaction |
| --- | --- | --- |
| `CON-42` | MUST close the stream on termination and MUST NOT send a message describing the reason. None is defined; `reject` is withdrawn. | — |
| `CON-44` | MUST NOT terminate a manually configured outbound connection for a protocol violation alone (`MSG-02`). | safety |

## 6. Connection roles

A node assigns a role to each outbound connection. The role is not negotiated and is not visible in any message, but it determines observable behaviour, and a peer MUST tolerate every behaviour below.

| Role | Observable behaviour |
| --- | --- |
| Full relay | Address, block and transaction relay. |
| Block relay only | Sends `relay` false. Relays blocks. No address record, no `getaddr`, no transaction traffic; ignores address messages received. |
| Reachability probe | Completes the handshake and terminates immediately. |
| Address solicitation | Completes the handshake, sends `getaddr`, terminates once a multi-record address message arrives or a bounded time elapses. |
| Private broadcast | Completes the handshake, announces and delivers one or more transactions it originated, then terminates. Used over privacy networks. |
| Manual | As full relay, exempt from automatic termination (`CON-44`). |

| ID | Rule | Reaction |
| --- | --- | --- |
| `CON-45` | MUST tolerate a peer that terminates immediately after the handshake, repeatedly, and MUST NOT treat its address as unreachable or hostile. | safety |
| `CON-46` | MUST NOT send a transaction announcement or a `tx` to a peer whose `version` carried `relay` false, until that peer enables relay by another defined means. | terminate |

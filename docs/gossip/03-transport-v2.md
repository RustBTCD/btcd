# 03. Encrypted transport, version 2

Capability tag: `[ENC]`.

The version 2 transport carries the same messages as version 1 inside an authenticated encrypted channel whose byte stream carries no fixed plaintext marker. BIP 324 is authoritative for the key agreement, key derivation and cipher construction. This module specifies what is observable: how the transport is selected and distinguished from version 1, the framing limits, and fallback.

## 1. Properties

The transport provides confidentiality and integrity against a passive observer and against an active attacker who cannot perform a key exchange with both parties. It provides **no authentication of identity**.

| ID | Rule | Reaction |
| --- | --- | --- |
| `T2-01` | MUST NOT treat a completed version 2 session as evidence of the peer's identity, and MUST NOT relax any other rule because a connection is encrypted. | safety |

## 2. Selection

An `[ENC]` initiator attempts version 2 on outbound connections whose destination advertises `NODE_P2P_V2`, and on destinations whose service flags are unknown, relying on section 6 for fallback.

| ID | Rule | Reaction |
| --- | --- | --- |
| `T2-02` | MUST NOT advertise `NODE_P2P_V2` unless it accepts version 2 sessions on its listening port. | deprioritise (peers open sessions that fail and reconnect with version 1 only under section 6) |
| `T2-04` | With `[LSN]`, MUST accept both transports on the same listening port and distinguish them as in section 3. | — |

## 3. Distinguishing the transports

Define the **version 1 prefix** as the 16 bytes consisting of the network magic followed by `76 65 72 73 69 6f 6e 00 00 00 00 00` (`version` padded to 12 bytes). A version 2 public key is indistinguishable from random.

| ID | Rule | Reaction |
| --- | --- | --- |
| `T2-06` | As responder, on the first received byte that differs from the version 1 prefix, MUST classify the session as version 2 and begin transmitting its own key immediately, without waiting for the rest of the peer's key. | terminate (handshake deadline) |
| `T2-07` | As responder, when all 16 bytes match the prefix, MUST classify the session as version 1, deliver those bytes to the version 1 parser as the start of the stream, and MUST NOT transmit a public key. | terminate |
| `T2-08` | As responder, MUST NOT transmit any byte before classifying the session. | terminate (a byte sent early corrupts a version 1 stream) |
| `T2-09` | As initiator, MUST NOT transmit a public key whose bytes 4 through 15 equal the corresponding bytes of the version 1 prefix; MUST generate a new key instead. | terminate (the responder treats it as a peer on a different network) |

A responder need buffer at most 16 bytes to classify; buffering more makes the version 1 fallback unimplementable, because the buffered bytes must be replayable into the version 1 parser.

## 4. Session establishment

In each direction:

```
  64-byte public key ‖ garbage (0 to 4095 bytes) ‖ 16-byte garbage terminator ‖ version packet ‖ application packets
```

The initiator sends its key and garbage on connecting. The responder sends its key and garbage on classifying the session as version 2. Each side derives session keys from both public keys, then sends its terminator and version packet. The terminator is key-derived and differs per session and direction.

| ID | Rule | Reaction |
| --- | --- | --- |
| `T2-10` | MUST NOT send more than 4095 bytes of garbage. | terminate |
| `T2-11` | MUST terminate if the peer's garbage terminator does not appear within the 4111 bytes following the peer's public key. | — |
| `T2-12` | MUST authenticate the garbage it sent as associated data of its first packet, and MUST verify the peer's garbage as associated data of the first packet received. | terminate |
| `T2-13` | MUST send a version packet as its first packet, with empty contents. | ignore (contents) |
| `T2-14` | MUST accept a version packet of any length up to the packet limit and MUST ignore its contents. | — |

## 5. Packets

| Part | Size | Description |
| --- | --- | --- |
| length | 3 | Encrypted, little-endian, length of the contents. |
| ciphertext | 1 + contents | Encryption of a one-byte header followed by the contents. |
| authentication tag | 16 | |

The header byte defines one bit, `0x80`, the **ignore bit**. A packet with it set is a **decoy** and carries no message.

| ID | Rule | Reaction |
| --- | --- | --- |
| `T2-16` | MUST discard a decoy without interpreting its contents, and MUST count it for rekeying. | — |
| `T2-17` | MAY send decoys at any point after its garbage terminator. | — |
| `T2-18` | MUST NOT send a packet whose contents exceed 4,000,013 bytes, and MUST terminate on receiving a length above that bound. | terminate |
| `T2-19` | MUST terminate when a packet fails authentication. There is no resynchronisation. | — |
| `T2-20` | MUST rekey both ciphers after every 224 packets in each direction, counting decoys and the version packet. | terminate (authentication fails at packet 225) |

## 6. Message type encoding

The first byte of a packet's contents selects the form. A non-zero byte is a **short identifier** (table). `0x00` introduces the **long form**: the next 12 bytes are the message type as in the version 1 `command` field.

| ID | Type | ID | Type | ID | Type |
| --- | --- | --- | --- | --- | --- |
| 1 | `addr` | 11 | `getdata` | 21 | `tx` |
| 2 | `block` | 12 | `getheaders` | 22 | `getcfilters` |
| 3 | `blocktxn` | 13 | `headers` | 23 | `cfilter` |
| 4 | `cmpctblock` | 14 | `inv` | 24 | `getcfheaders` |
| 5 | `feefilter` | 15 | `mempool` | 25 | `cfheaders` |
| 6 | `filteradd` | 16 | `merkleblock` | 26 | `getcfcheckpt` |
| 7 | `filterclear` | 17 | `notfound` | 27 | `cfcheckpt` |
| 8 | `filterload` | 18 | `ping` | 28 | `addrv2` |
| 9 | `getblocks` | 19 | `pong` | | |
| 10 | `getblocktxn` | 20 | `sendcmpct` | | |

Types without a short identifier, including `version`, `verack`, `sendaddrv2`, `wtxidrelay`, `sendheaders`, `feature` and `getaddr`, use the long form. A node uses the short form wherever one exists.

| ID | Rule | Reaction |
| --- | --- | --- |
| `T2-21` | MUST decode both forms, applying `T1-10` to a long-form type. | discard |
| `T2-23` | MUST discard a packet whose short identifier it does not recognise, and MUST NOT terminate. Identifiers above 28 are reserved. | — |
| `T2-25` | MUST discard a packet whose contents are empty. | — |

## 7. Fallback to version 1

A version 1 responder sees an initiator's key as a malformed header with the wrong magic and terminates without sending anything.

| ID | Rule | Reaction |
| --- | --- | --- |
| `T2-26` | On receiving bytes that are neither a version 2 handshake it can process nor the version 1 prefix, MUST close the connection **without transmitting any byte**. | deprioritise (any byte sent, including a `version` message, suppresses the peer's version 1 retry, and the node is unreachable to it) |
| `T2-27` | As initiator, MUST retry a failed version 2 attempt once using version 1 when all of: it transmitted at least 24 bytes; it received nothing; the connection then closed. It MUST NOT count the failed attempt against the address. | safety |
| `T2-29` | MUST NOT downgrade to version 1 after a version 2 session has produced any packet, and MUST NOT accept plaintext framing on a session classified as version 2. | safety |
| `T2-30` | MUST complete the transport handshake and the `version`/`verack` exchange within the single deadline of `CON-40`. | terminate |

Completing a version 2 session establishes no application state: the handshake of [04-connection.md](04-connection.md) follows exactly as on version 1.

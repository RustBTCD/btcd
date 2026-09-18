# 02. Plaintext transport, version 1

The version 1 transport frames messages in cleartext. It provides no confidentiality, no integrity against an active attacker and no authentication; its checksum detects accidental corruption only.

Every node MUST implement this transport. The version 2 transport ([03-transport-v2.md](03-transport-v2.md)) is optional and is detected before any message is exchanged.

## 1. Frame layout

A frame is a 24-byte header followed by the payload.

| Field | Size | Type | Description |
| --- | --- | --- | --- |
| `magic` | 4 | bytes | Network identifier ([90-parameters.md](90-parameters.md) section 1). |
| `command` | 12 | bytes | Message type in ASCII, padded on the right with `0x00`. |
| `length` | 4 | `uint32` | Payload length in bytes. |
| `checksum` | 4 | bytes | First four bytes of `SHA256d(payload)`. |
| `payload` | `length` | bytes | Message body. |

The checksum of an empty payload is `5d f6 e0 e2`.

## 2. Sending

| ID | Rule | Reaction |
| --- | --- | --- |
| `T1-01` | MUST set `magic` to the identifier of the network it operates on. | terminate |
| `T1-02` | MUST encode `command` as ASCII in the range `0x20` to `0x7E`, padded to 12 bytes with `0x00`. | discard |
| `T1-03` | MUST NOT send a frame whose `length` exceeds 4,000,000. | terminate (on reading the header, before the payload arrives) |
| `T1-04` | MUST set `checksum` correctly. | discard (a silent stall rather than an error) |
| `T1-05` | MUST write each frame completely before beginning the next. | terminate (the stream desynchronises) |
| `T1-15` | As initiator, MUST write the header of a `version` message as the first bytes on the connection, so that the first 16 bytes are the magic followed by `76 65 72 73 69 6f 6e 00 00 00 00 00`. | terminate (a responder supporting version 2 classifies the connection as encrypted and the handshake times out) |

## 3. Receiving

| ID | Rule | Reaction |
| --- | --- | --- |
| `T1-06` | MUST validate `magic` before interpreting the rest of the header, and MUST terminate on mismatch. | — |
| `T1-07` | MUST terminate on a `length` above 4,000,000, without reading the payload. | — |
| `T1-08` | MUST NOT allocate the full `length` before the bytes have arrived (`ENC-06`). | safety |
| `T1-09` | MUST verify `checksum` before acting on a payload, and MUST discard the message on mismatch. | — |
| `T1-10` | MUST discard a message whose `command` contains a byte outside `0x20` to `0x7E` before the first `0x00`, or a non-zero byte after it. | — |
| `T1-12` | MUST discard a message whose payload cannot be parsed, and MUST NOT terminate for that alone, except where the module owning the message says otherwise. | — |

Unknown message types are handled by `MSG-10`. Trailing bytes after a parsed payload are ignored unless a message's definition says otherwise.

## 4. Flow control

The transport relies on the underlying stream for flow control.

| ID | Rule | Reaction |
| --- | --- | --- |
| `T1-16` | MUST continue reading from a connection while it has data queued to write to it. | safety (peers stop processing a connection's input once their unsent data for it exceeds a bound; two nodes that each stop reading while writing deadlock until the inactivity deadline) |

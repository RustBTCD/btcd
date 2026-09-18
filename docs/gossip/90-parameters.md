# 90. Parameters

Every constant referenced by the normative modules. This module is normative for values: where a module and this table disagree, this table is authoritative.

## 1. Networks

| Network | Magic | Default port |
| --- | --- | --- |
| Main | `f9 be b4 d9` | 8333 |
| Test (v3) | `0b 11 09 07` | 18333 |
| Test (v4) | `1c 16 3f 28` | 48333 |
| Signet (default) | `0a 03 cf 40` | 38333 |
| Regression test | `fa bf b5 da` | 18444 |

A signet is parameterised by a challenge script and its magic is derived from it; a custom signet has different magic. The port is a default only: an address record carries its own port and a node MUST honour it.

## 2. Protocol versions

| Value | Introduced | Significance |
| --- | --- | --- |
| 31800 | — | Minimum accepted (`CON-16`). |
| 60000 | BIP 31 | `ping` carries a nonce; `pong` expected. |
| 70012 | BIP 130 | `sendheaders`. |
| 70013 | BIP 133 | `feefilter`. |
| 70014 | BIP 152 | `sendcmpct`. |
| 70015 | — | Compact block relay before full validation permitted toward peers at or above this (`CMP-25`). |
| 70016 | BIP 155, BIP 339 | `sendaddrv2`, `wtxidrelay`. |
| 70017 | BIP 434 | `feature`. The value this specification recommends (`CON-17`); the maximum permitted (`CON-34`). |

## 3. Size and count limits

Normative through `ENC-08`. The reaction is what a peer does on receipt of a violation.

| Limit | Value | Applies to | Reaction |
| --- | --- | --- | --- |
| Frame payload | 4,000,000 bytes | every message, version 1 | terminate, before reading the payload |
| Packet contents | 4,000,013 bytes | version 2 | terminate |
| Compact size | 33,554,432 | any length or count prefix | discard |
| Address records | 1000 | `addr`, `addrv2` | terminate and record |
| Inventory vectors | 50,000 | `inv`, `getdata`, `notfound` | terminate and record |
| Headers | 2000 | `headers` | terminate and record |
| Locator hashes | 101 | `getheaders`, `getblocks` | terminate |
| User agent | 256 bytes | `version` | discard the whole `version` |
| Address bytes | 512 | `addrv2` entry | discard the whole message |
| Feature identifier | 4 to 80 bytes | `feature` | terminate |
| Feature data | 512 bytes | `feature` | terminate |
| Garbage | 4095 bytes | version 2 handshake | terminate |
| Blocks below a limited peer's tip | 288, minus 2 for races | `getdata` to `NODE_NETWORK_LIMITED` | terminate |

Self-imposed limits a node applies to its own requests, not enforced by peers:

| Limit | Value |
| --- | --- |
| Inventory vectors per `getdata` sent | about 1000 |
| Blocks requested from one peer at once | 16 |
| Headers per announcement | about 8 |
| High-bandwidth compact block peers | 3 |
| Automatic outbound connections | 8 full relay, 2 block relay only |
| Announcements tracked per peer | about 5000 |
| Transaction requests in flight per peer before delaying | about 100 |

## 4. Ports to avoid

`SEL-03`. A port configured by the node's own operator is exempt.

1, 7, 9, 11, 13, 15, 17, 19, 20, 21, 22, 23, 25, 37, 42, 43, 53, 69, 77, 79, 87, 95, 101, 102, 103, 104, 109, 110, 111, 113, 115, 117, 119, 123, 135, 137, 139, 143, 161, 179, 389, 427, 465, 512, 513, 514, 515, 526, 530, 531, 532, 540, 548, 554, 556, 563, 587, 601, 636, 989, 990, 993, 995, 1719, 1720, 1723, 2049, 3306, 3389, 3659, 4045, 5060, 5061, 5432, 5900, 6000, 6566, 6665, 6666, 6667, 6668, 6669, 6697, 10080, 27017.

## 5. Deadlines and rates enforced by peers

A node MUST meet these.

| Deadline | Value | Measured from | Rule |
| --- | --- | --- | --- |
| Handshake completion, including transport | 60 s | stream established | `CON-40` |
| First byte in each direction | 60 s | stream established | `CON-41` |
| Inactivity | 20 min | last byte received | `CON-41` |
| Unanswered `ping` | 20 min | `ping` sent | `CON-39` |
| Headers from the initial synchronisation peer | 15 min + 1 ms per header expected | synchronisation started | `BLK-01` |
| Lagging chain | about 20 min, then about 2 min after an explicit `getheaders` | connection established | `SEL-22` |
| Block delivery, single block blocking progress | a few seconds | request sent | `BLK-31` |
| Block delivery, batch | about 10 min, plus about 5 min per peer downloaded from in parallel | request sent | `BLK-31` |
| Transaction delivery | about 1 min | request sent | `TXR-22` |

| Rate | Value | Rule |
| --- | --- | --- |
| Address records processed per peer | 1 per 10 s average, 1000 on the first message, plus 1000 per `getaddr` sent | `ADR-06` |
| Transaction announcements tracked per peer | about 5000 | `TXR-21` |
| Historical block serving | operator-set upload target; blocks older than about a week refused once reached | `BLK-34` |

## 6. Delays a peer must tolerate

| Delay | Magnitude | Rule |
| --- | --- | --- |
| `getaddr` response | tens of seconds | `ADR-29` |
| Transaction announcement after acceptance | seconds, randomised | `TXR-06` |
| Transaction request after announcement | about 2 s per applicable condition, randomised | `TXR-19` |
| Termination immediately after handshake | none | `CON-45` |

## 7. Intervals for originated traffic

Conventions, not enforced. Randomised where the rule says so.

| Traffic | Average interval | Rule |
| --- | --- | --- |
| `ping` | 2 min | `CON-36` |
| Address batch per peer | 30 s, exponential | `ADR-34` |
| Transaction announcement batch, accepted connections | 5 s, exponential, shared | `TXR-07` |
| Transaction announcement batch, initiated connections | 2 s, exponential | `TXR-07` |
| Self-announcement | 24 h, exponential | `ADR-09` |
| Relay destination rotation | 24 h | `ADR-16` |
| `getaddr` response stability | about a day, randomised | `ADR-33` |
| Reachability probe | about 2 min | `SEL-10` |
| `feefilter` re-send | about 10 min, randomised | `TXR-12` |

## 8. Other constants

| Constant | Value | Rule |
| --- | --- | --- |
| Blocks a limited peer must serve | 288 | `BLK-30` |
| Depth within which a limited peer is acceptable as an outbound source | 144 | `CON-49` |
| Rekey interval | 224 packets | `T2-20` |
| Version 1 prefix length | 16 bytes | `T2-06` |
| Bytes an initiator must have sent before a version 1 retry | 24 | `T2-27` |
| Address relay fan-out | 2 peers (1 for an address the node cannot reach) | `ADR-16` |
| Records per message eligible for relay | 10 | `ADR-18` |
| Record age eligible for relay | 10 min | `ADR-18` |
| Future timestamp tolerance | 10 min | `ADR-22` |
| `getaddr` response size | 1000 records, at most about a quarter of the store | `ADR-32` |
| `getblocks` response | 500 hashes | `MSG-20` |
| Side-chain block serving age | about 30 days | `BLK-36` |
| Compact block full-block fallback depth | more than 5 blocks (`getdata`), more than 10 (`getblocktxn`) | `CMP-18` |

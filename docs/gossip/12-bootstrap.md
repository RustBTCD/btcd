# 12. Bootstrap

A node with no knowledge of any peer cannot use address gossip. This module specifies how that circularity is broken and what constraints apply to the sources used.

## 1. Sources

A node obtains initial addresses from some combination of:

1. Addresses retained from previous operation. Preferred: consulting anything else discloses that the node is starting.
2. Addresses supplied by the operator.
3. A **seed service**: an out-of-band service returning addresses of listening nodes, conventionally over the Domain Name System.
4. **Fixed seeds**: addresses distributed with the implementation. Used last, only for networks on which the node holds no address at all.

A node consults a few seed services at a time rather than all of them, and stops once it has established a couple of outbound connections and is receiving addresses by gossip. Addresses from seeds are back-dated by days, and fixed seeds by weeks, so that they rank below anything the node has verified itself.

| ID | Rule | Reaction |
| --- | --- | --- |
| `BST-01` | MUST treat every address from any bootstrap source as unverified, exactly as one received by gossip. | safety |
| `BST-02` | MUST NOT allow any single bootstrap source, or any single peer, to determine its entire peer set. | safety |
| `BST-07` | MUST attribute all addresses from one seed to a single source for the purpose of `ADR-25`. | safety |
| `BST-09` | When querying a seed over the Domain Name System, SHOULD request only nodes offering the required service flags by prefixing the name with `x` and the flags in hexadecimal, for example `x9.` for `NODE_NETWORK` with `NODE_WITNESS`, and MUST fall back to the unprefixed name when the seed does not support it. | — |
| `BST-10` | With `[PRV]`, when every connection is made through an anonymising proxy, MUST NOT resolve names locally. It MAY instead connect to a seed's host as an ordinary peer and solicit addresses with `getaddr`. | safety |

## 2. Address solicitation connections

A node MAY connect to an address solely to solicit addresses from it (the address solicitation role, [04-connection.md](04-connection.md) section 6).

| ID | Rule | Reaction |
| --- | --- | --- |
| `BST-14` | MUST terminate an address solicitation connection once it has received a multi-record address message, or after a bounded period if none arrives. | safety |
| `BST-15` | MUST NOT treat a single-record address message as the solicited response; it is the peer's self-announcement (`ADR-10`). | safety |

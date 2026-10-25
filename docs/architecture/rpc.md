# RPC

An RPC is a command or query addressed to exactly one daemon. The caller is the operator or another daemon;
the transport and the handler are the same for both.

## Semantics

- **Name:** `<daemon>.<rpc>`. The daemon serving it declares it in its manifest with a parameter schema and an
  `operator` flag ([manifest.md](manifest.md#schema)).
- **Request/response:** the caller sends a request and gets exactly one response: `result` or `error`. Every RPC is
  answered, including commands with nothing to report, which return `{}`. A caller that does not need the outcome may
  send the request and not wait for the response.
- **Single owner:** `rbtcd` routes the request to the daemon named in the method. There is no fan-out.
- **Permissions:** checked by `rbtcd` before forwarding. A daemon may call only the RPCs listed in its own `calls`.
  The operator may call only RPCs with `operator: true` and `node.*`.
- **Caller identity:** the handler sees the caller in the `from` envelope member (`operator`, `node`, or a daemon name).
  Handlers MAY use it, for example to word errors for the operator, but MUST NOT use it as a substitute for the
  permission checks.
- **Availability:** a request to a daemon that has not sent `bus.ready`, or is shutting down, fails at once with
  `unavailable` or `shutting_down`.
- **Concurrency:** a daemon decides itself whether it handles requests one at a time or concurrently.
  Each daemon's README states its choice. Daemons that change persistent state (`chain`, `utxo`) serialize their writes.

### When to use what

| The caller wants to… | Use |
|---|---|
| tell everyone something happened | event |
| make one daemon do something and know the outcome | RPC |
| make one daemon do something and not wait | RPC, without waiting for the response |
| read another daemon's state | RPC |
| let unknown parties veto an action | hook |

## Operator socket

- Unix stream socket `<datadir>/<network>/rbtc.sock`, mode `0600`. Anyone who can open it is a full operator;
  there is no other authentication in v0.1.
- Same framing and envelope as the bus. Only requests are accepted. A notification or response from a client closes the
  connection.
- A client may send several requests on one connection without waiting for each response. Responses carry the request id
  and may come back in any order.
- At most 16 clients are connected at once. All operator clients together count as one caller for the in-flight limit.

## CLI

```
rbtc [global options] <daemon> <rpc> [arguments...] [--<param> <value>]... [--<flag>]...
rbtc [global options] help [<daemon> [<rpc>]]
```

Global options, which must come before `<daemon>`:

| Option | Default | Meaning |
|---|---|---|
| `--datadir <dir>` | `~/.rbtc` | Data directory. |
| `--network <net>` | `main` | Selects `<datadir>/<network>/rbtc.sock`. |
| `--socket <path>` | — | Use this socket path directly. |
| `--timeout <secs>` | `60` | Client-side timeout; `0` waits forever. |

Argument mapping:

1. The CLI first calls `node.help {"daemon": <daemon>, "rpc": <rpc>}` to get the parameter schema.
   The CLI has no built-in knowledge of any daemon, so a new daemon's RPCs work without changes to the CLI.
2. Positional arguments fill parameters in declared order.
3. `--name value` or `--name=value` sets a parameter by name. Dashes and underscores are interchangeable (`--max-count`
   is `max_count`).
4. A `boolean` parameter given as `--name` alone is `true`.
5. Values are converted by declared type ([manifest.md](manifest.md#schema)). Unknown parameter names, missing required
   parameters, a parameter set twice, or a value of the wrong type are usage errors, reported before anything is sent.

Output:

- On success, the `result` is printed to stdout as indented JSON.
- On error, `error <code> <name>: <message>` is printed to stderr, followed by `data` if present.

Exit codes: `0` success, `1` RPC error, `2` usage error, `3` cannot connect or timed out.

Examples:

```
rbtc chain info
rbtc chain block 850000
rbtc chain raw_block 00000000000000000002a7c4c1e48d76c5a37902165a270156b7a8d72728a054
rbtc chain tx 1a2b…f0 0000…a054
rbtc utxo coin 1a2b…f0 1
rbtc net peers
rbtc help chain
rbtc node stop
```

## Error codes

Errors use a small shared set of codes. The machine-readable detail goes into `data.reason`.
Where Bitcoin Core has a reject reason for the same failure (`bad-txns-inputs-missingorspent`, `bad-blk-merkle`,
`high-hash`, …), Core's string is used.

| Code | Name | Meaning |
|---|---|---|
| -32700 | `parse_error` | Not valid JSON. |
| -32600 | `invalid_request` | Valid JSON but not a valid request. |
| -32601 | `method_not_found` | No such daemon or RPC. |
| -32602 | `invalid_params` | Parameters do not match the schema or are malformed (bad hex, bad hash). |
| -32603 | `internal_error` | The handler failed unexpectedly. `message` contains the source error text. |
| 1 | `not_permitted` | The caller may not call this RPC. |
| 2 | `unavailable` | The target daemon is not ready. |
| 3 | `busy` | In-flight limit reached; retry later. |
| 4 | `shutting_down` | The node is stopping. |
| 5 | `invalid_manifest` | Answer to `bus.init` when the registry is invalid. `data` lists the problems. |
| 10 | `not_found` | The requested object does not exist. |
| 11 | `invalid` | The object breaks consensus rules. `data.reason` is the reject reason. |
| 12 | `rejected` | The request is valid but refused, for example by a hook. `data.reason` says why. |
| 14 | `precondition_failed` | The request does not fit the daemon's current state, for example connecting a block that does not extend the UTXO tip. Indicates a bug in the caller. |

## Catalogue

`op` = callable by the operator.

| RPC | op | Purpose |
|---|---|---|
| `node.status` | ✓ | Daemons, their state and pids, uptime, network |
| `node.help` | ✓ | RPC descriptions and parameter schemas |
| `node.registry` | ✓ | Full registry: events, hooks, subscribers, RPCs, call graph |
| `node.stop` | ✓ | Orderly shutdown |
| `net.info` | ✓ | Listening state, connection counts, traffic totals |
| `net.peers` | ✓ | Connected peers |
| `net.connect` | ✓ | Open a manual connection |
| `net.disconnect` | ✓ | Close a connection |
| `net.send` |  | Send a P2P message to a peer |
| `net.misbehaving` |  | Disconnect and discourage a peer |
| `chain.info` | ✓ | Tip, best header, IBD state |
| `chain.block` | ✓ | A stored block, decoded |
| `chain.raw_block` | ✓ | A stored block in consensus encoding |
| `chain.header` | ✓ | A header with its index data |
| `chain.tips` | ✓ | All known chain tips (Core's `getchaintips`) |
| `chain.tx` | ✓ | A transaction from a given block |
| `chain.submit_headers` |  | Validate and store headers |
| `chain.submit_block` |  | Validate, store and possibly connect a block |
| `chain.locator` |  | Block locator for `getheaders` |
| `chain.missing_blocks` |  | Blocks to download next |
| `chain.headers_after` |  | Headers to serve a peer's `getheaders` |
| `utxo.info` | ✓ | UTXO tip and set statistics |
| `utxo.coin` | ✓ | One unspent output (Core's `gettxout`) |
| `utxo.connect_block` |  | Validate inputs and scripts and apply a block |
| `utxo.disconnect_block` |  | Undo the tip block |
| `block-sync.status` | ✓ | Sync state, peers and blocks in flight |

Full schemas: [main](../daemons/main/README.md#rpcs), [net](../daemons/net/rpc.md), [chain](../daemons/chain/rpc.md),
[utxo](../daemons/utxo/rpc.md), [block-sync](../daemons/block-sync/rpc.md).

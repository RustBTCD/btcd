# Lifecycle

Startup, shutdown, failure handling, configuration and the data directory. Sequence diagrams are in [flows/startup-and-shutdown.md](../flows/startup-and-shutdown.md).

## Startup

1. `rbtcd` parses its command line (`--config`, `--datadir`, `--network`), reads the config file and validates the `[node]` section. Other sections are only checked to be TOML tables.
2. It creates `<datadir>/<network>/` and one directory per configured daemon inside it, and takes an exclusive lock on `<datadir>/<network>/.lock`. A second node on the same data directory fails here.
3. It opens the operator socket. Until a daemon is ready, operator requests to it fail with `unavailable`; `node.*` works from this point on.
4. It spawns every daemon listed in `[node] daemons`, each in its own process group, with the bus socket on fd 3 and the log level in the environment variable `RBTC_LOG`.
5. It waits until every daemon has sent `bus.init` (`init_timeout_secs`).
6. It validates the registry as a whole ([manifest.md](manifest.md#validation)). On failure it answers every pending `bus.init` with `invalid_manifest`, logs all problems, and stops.
7. It initialises daemons **one at a time in topological order** of the dependency graph (dependencies first; ties are broken by the order in `[node] daemons`). For each daemon it answers `bus.init` with the network, data directory and config section, then waits for `bus.ready` (`ready_timeout_secs`). During its initialisation a daemon may call RPCs of daemons that are already ready (all its dependencies are, by construction). It must not emit events.
8. When every daemon is ready, `rbtcd` sends `bus.start` to all of them. From now on events flow, and sources start taking input: `net` starts listening and connecting.

The v0.1 order is `utxo`, `net`, `chain`, `block-sync`, `tx-relay`. `chain` reconstructs the active chain from `utxo`'s tip during its initialisation ([storage.md](storage.md#startup-reconciliation)). `net` opens no connection before `bus.start`.

### Running a subset of daemons

`[node] daemons` may list any subset of daemons whose registry validates. Without `tx-relay` the node relays no transactions; events without subscribers are dropped by `rbtcd`.

## Shutdown

Triggered by `node.stop`, by `SIGINT` or `SIGTERM` to `rbtcd`, or by a failure (below).

1. `rbtcd` stops routing events: queued and new events are dropped. New operator requests fail with `shutting_down`.
2. It sends `bus.shutdown` to daemons **one at a time in reverse topological order** (`tx-relay`, `block-sync`, `chain`, `net`, `utxo`). Each daemon stops starting new work, finishes or abandons current work, flushes its storage, answers, and exits. `rbtcd` waits up to `shutdown_timeout_secs` for the answer and 5 seconds more for the process to exit, then kills it.
3. RPCs to a daemon that has already been shut down fail with `shutting_down`. Daemons later in the order still answer RPCs from those before them, so a daemon that is shutting down can finish a call (for example `chain` completing a `utxo.connect_block`).
4. `rbtcd` removes the operator socket, releases the lock, and exits with code 0, or 1 if the shutdown was caused by a failure.

A node that is killed instead of shut down restarts into a consistent state ([storage.md](storage.md#crash-cases)).

## Failures

`rbtcd` treats each of these as a failure of the daemon involved:

- the process exits or its bus connection closes outside shutdown;
- a protocol violation ([bus-protocol.md](bus-protocol.md#message-classification));
- `bus.init` or `bus.ready` not received in time.

v0.1 is **fail-stop**: `rbtcd` logs the failure with the daemon's last stderr lines, shuts the whole node down, and exits with code 1. Individual daemons are not restarted.

If `rbtcd` itself dies, every daemon sees end-of-file on fd 3 and exits immediately.

Daemons run in their own process group, so `Ctrl-C` in a terminal reaches only `rbtcd`, which then runs the orderly shutdown.

## Configuration

One TOML file, by default `<datadir>/rbtc.toml`, or the path given with `--config`. A missing file means all defaults.

`[node]` belongs to `rbtcd`. Every other top-level table is named after a daemon and is passed to that daemon in the `bus.init` response. Each daemon applies its defaults and rejects unknown keys. A table for a daemon that is not configured to run is an error.

```toml
[node]
network = "main"                 # main | testnet4 | signet | regtest; --network overrides
daemons = ["utxo", "net", "chain", "block-sync", "tx-relay"]
log_level = "info"               # passed to daemons as RBTC_LOG
log_file = "rbtc.log"            # relative to <datadir>/<network>/; "" logs to stderr only
init_timeout_secs = 10
ready_timeout_secs = 600         # chain may load a large block index
shutdown_timeout_secs = 60

[node.bus]
max_frame_mib = 64
event_window_mib = 32
queue_high_water_mib = 64
rpc_in_flight = 64

[node.paths]                     # optional executable overrides; default: rbtc-<name> next to rbtcd
# net = "/opt/rbtc/bin/rbtc-net"

[net]                            # see daemons/net/README.md
[chain]                          # see daemons/chain/README.md
[utxo]                           # see daemons/utxo/README.md
[block-sync]                     # see daemons/block-sync/README.md
[tx-relay]                       # see daemons/tx-relay/README.md
```

Configuration is read once at startup. Changing it requires a restart.

## Data directory

```
<datadir>/                       default ~/.rbtc
├── rbtc.toml
└── <network>/                   main, testnet4, signet, regtest
    ├── .lock
    ├── rbtc.sock                operator socket, mode 0600
    ├── rbtc.log
    ├── net/
    │   └── addresses.json       store addresses
    ├── chain/
    │   ├── index/               store index (RocksDB)
    │   └── blocks/              store blocks (blk00000.dat, …)
    ├── utxo/
    │   └── chainstate/          store chainstate (RocksDB)
    ├── block-sync/              empty in v0.1
    └── tx-relay/                empty in v0.1
```

A daemon MUST NOT read or write another daemon's directory.

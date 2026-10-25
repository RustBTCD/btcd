# rbtc

A Bitcoin full node written in Rust.

- **Compatible with Bitcoin Core, not a copy of it.** Consensus rules and P2P behaviour must match Core exactly.
  Internal design (multi-process architecture, bus, storage, operator interface) is our own, and so are features Core
  does not have. Do not port Core's structure just because Core does it that way.
- **The specification in `docs/` is the source of truth.** Before working on anything, read `docs/README.md` (it gives
  the reading order), then the docs of every daemon and flow your change touches.
- **Reference sources go in `.ref/`.** When you need to check how Bitcoin Core or another project does something, clone
  it into `.ref/<name>` (shallow is enough, for example
  `git clone --depth 1 https://github.com/bitcoin/bitcoin .ref/bitcoin`) and read it there. Useful candidates:
  Bitcoin Core, the BIPs (`bitcoin/bips`), rust-bitcoin, `rust-bitcoinkernel`, and Core Lightning
  (`ElementsProject/lightning`), whose plugin model inspired ours (its plugin docs are in
  `doc/developers-guide/plugin-development/`). `.ref/` is git-ignored; cloning into it is allowed despite the git
  rules, because it does not touch this repository. Never edit files in `.ref/` and never depend on them from our code.

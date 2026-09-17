//! Per-chain constants the node needs before consensus rules exist.
//!
//! Values mirror Bitcoin Core's `kernel/chainparams.cpp`.

use bitcoin::Network;
use serde::Deserialize;

/// Which Bitcoin chain the node follows. Testnet3 is deliberately not supported.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Chain {
    #[default]
    Main,
    Testnet4,
    Signet,
    Regtest,
}

impl Chain {
    pub fn network(self) -> Network {
        match self {
            Chain::Main => Network::Bitcoin,
            Chain::Testnet4 => Network::Testnet4,
            Chain::Signet => Network::Signet,
            Chain::Regtest => Network::Regtest,
        }
    }

    /// Chain name as used by Bitcoin Core's `-chain` option. Also names the data subdirectory.
    pub fn name(self) -> &'static str {
        match self {
            Chain::Main => "main",
            Chain::Testnet4 => "testnet4",
            Chain::Signet => "signet",
            Chain::Regtest => "regtest",
        }
    }

    /// Bitcoin Core `nDefaultPort`.
    pub fn default_port(self) -> u16 {
        match self {
            Chain::Main => 8333,
            Chain::Testnet4 => 48333,
            Chain::Signet => 38333,
            Chain::Regtest => 18444,
        }
    }

    /// Bitcoin Core `vSeeds`. Regtest has none.
    pub fn dns_seeds(self) -> &'static [&'static str] {
        match self {
            Chain::Main => &[
                "dnsseed.bluematt.me",
                "seed.bitcoin.jonasschnelli.ch",
                "seed.btc.petertodd.net",
                "seed.bitcoin.sprovoost.nl",
                "dnsseed.emzy.de",
                "seed.bitcoin.wiz.biz",
                "seed.mainnet.achownodes.xyz",
            ],
            Chain::Testnet4 => &[
                "seed.testnet4.bitcoin.sprovoost.nl",
                "seed.testnet4.wiz.biz",
            ],
            Chain::Signet => &[
                "seed.signet.bitcoin.sprovoost.nl",
                "seed.signet.achownodes.xyz",
            ],
            Chain::Regtest => &[],
        }
    }
}

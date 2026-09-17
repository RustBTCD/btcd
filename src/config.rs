//! Node configuration loaded from a TOML file.
//!
//! Every section and key is optional; missing values fall back to defaults.
//! Unknown keys are rejected so typos fail loudly.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::chain_params::Chain;
use crate::p2p::P2pConfig;
use crate::storage::StorageConfig;

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Chain to follow: `main`, `testnet4`, `signet` or `regtest`.
    pub chain: Chain,
    /// Most detailed log level printed.
    pub log_level: LogLevel,
    pub storage: StorageConfig,
    pub p2p: P2pConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Error,
    Warn,
    #[default]
    Info,
    Debug,
    Trace,
}

impl From<LogLevel> for tracing::Level {
    fn from(level: LogLevel) -> Self {
        match level {
            LogLevel::Error => tracing::Level::ERROR,
            LogLevel::Warn => tracing::Level::WARN,
            LogLevel::Info => tracing::Level::INFO,
            LogLevel::Debug => tracing::Level::DEBUG,
            LogLevel::Trace => tracing::Level::TRACE,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("reading config file {}: {source}", path.display())]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("parsing config file {}: {source}", path.display())]
    Parse {
        path: PathBuf,
        source: toml::de::Error,
    },
}

impl Config {
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path).map_err(|source| ConfigError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        Self::from_toml(&text).map_err(|source| ConfigError::Parse {
            path: path.to_path_buf(),
            source,
        })
    }

    pub fn from_toml(text: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The example file documents the defaults; keep the two in sync.
    #[test]
    fn example_file_matches_defaults() {
        let example = Config::from_toml(include_str!("../rust-btcd.toml")).unwrap();
        assert_eq!(example, Config::default());
    }

    #[test]
    fn empty_file_gives_defaults() {
        assert_eq!(Config::from_toml("").unwrap(), Config::default());
    }

    #[test]
    fn partial_override_keeps_other_defaults() {
        let config = Config::from_toml("[storage]\nblock_cache_mib = 1024\n").unwrap();
        assert_eq!(config.storage.block_cache_mib, 1024);
        assert_eq!(config.storage.path, StorageConfig::default().path);
    }

    #[test]
    fn chain_and_p2p_settings_parse() {
        let config = Config::from_toml(
            "chain = \"regtest\"\nlog_level = \"debug\"\n[p2p]\npeers = [\"127.0.0.1:18444\"]\n",
        )
        .unwrap();
        assert_eq!(config.chain, Chain::Regtest);
        assert_eq!(config.log_level, LogLevel::Debug);
        assert_eq!(config.p2p.peers, vec!["127.0.0.1:18444".to_string()]);
        assert_eq!(config.p2p.max_outbound, P2pConfig::default().max_outbound);
    }

    #[test]
    fn unsupported_chains_are_rejected() {
        assert!(Config::from_toml("chain = \"testnet\"\n").is_err());
    }

    #[test]
    fn unknown_keys_are_rejected() {
        assert!(Config::from_toml("[storage]\nblock_cache = 1\n").is_err());
        assert!(Config::from_toml("[stroage]\n").is_err());
    }

    #[test]
    fn missing_file_is_a_read_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("missing.toml");
        let err = Config::from_file(&path).unwrap_err();
        assert!(matches!(err, ConfigError::Read { .. }));
        assert!(err.to_string().starts_with("reading config file "));
    }

    #[test]
    fn invalid_file_is_a_parse_error_with_the_toml_message() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.toml");
        std::fs::write(&path, "[storage]\ncache = 1\n").unwrap();
        let err = Config::from_file(&path).unwrap_err();
        assert!(matches!(err, ConfigError::Parse { .. }));
        assert!(err.to_string().contains("unknown field `cache`"));
    }
}

mod block;
mod chain_params;
mod config;
mod error;
mod header_chain;
mod p2p;
mod storage;
mod validation;

use std::io::IsTerminal;
use std::process::ExitCode;

use tracing::info;

use config::Config;
use error::Error;
use p2p::Manager;
use storage::Storage;

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            // Messages already include their cause, so the top-level message is enough.
            eprintln!("error: {err}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<(), Error> {
    let config = match std::env::args_os().nth(1) {
        Some(path) => Config::from_file(path)?,
        None => Config::default(),
    };
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::from(config.log_level))
        .with_target(false)
        .with_ansi(std::io::stdout().is_terminal())
        .init();

    // Each chain gets its own database, like Bitcoin Core's data directory layout.
    let mut storage_config = config.storage.clone();
    storage_config.path = config.storage.path.join(config.chain.name());
    let storage = Storage::open(&storage_config)?;
    info!(
        "following {}, data in {}",
        config.chain.name(),
        storage_config.path.display()
    );

    let manager = Manager::new(config.chain, config.p2p, storage).await?;
    tokio::select! {
        result = manager.run() => result?,
        _ = tokio::signal::ctrl_c() => info!("shutting down"),
    }
    Ok(())
}

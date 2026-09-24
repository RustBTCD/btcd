mod block;
mod config;
mod error;
mod storage;
mod wire;

use std::process::ExitCode;

use config::Config;
use error::Error;
use storage::Storage;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            // Messages already include their cause, so the top-level message is enough.
            eprintln!("error: {err}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Error> {
    let config = match std::env::args_os().nth(1) {
        Some(path) => Config::from_file(path)?,
        None => Config::default(),
    };

    let storage = Storage::open(&config.storage)?;
    let path = config.storage.path.display();
    match storage.chainstate().tip()? {
        Some(tip) => println!("storage at {path}, tip {tip}"),
        None => println!("storage at {path}, empty chain"),
    }
    Ok(())
}

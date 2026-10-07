use std::io;

use anyhow::{Result, anyhow};
use tracing::Level;

/// Print debug logs to stderr when `verbose` is set.
pub fn init(verbose: bool) -> Result<()> {
    if !verbose {
        return Ok(());
    }
    tracing_subscriber::fmt()
        .with_max_level(Level::DEBUG)
        .with_writer(io::stderr)
        .without_time()
        .try_init()
        .map_err(|error| anyhow!("failed to set up logging: {error}"))
}

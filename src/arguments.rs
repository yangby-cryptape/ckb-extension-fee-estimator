use std::{net::SocketAddr, path::PathBuf};

use clap::Parser;

use crate::error::{Error, Result};

#[derive(Debug, Parser)]
#[clap(name = "[Experimental] CKB Fee Estimator", version, author, about)]
pub(crate) struct Cli {
    /// The address of CKB RPC subscription.
    #[clap(long)]
    subscribe_addr: SocketAddr,
    /// The HTTP address which CKB Fee Estimator RPC service will listen on.
    #[clap(long)]
    listen_addr: SocketAddr,
    /// The directory where to store the caches.
    #[clap(hide = true, long, default_value = "data")]
    data_dir: PathBuf,
}

impl Cli {
    pub(crate) fn load() -> Result<Self> {
        let cli = Self::parse();
        let data_dir = cli.data_dir();
        if data_dir.exists() && !data_dir.is_dir() {
            Err(Error::config("data-dir exists but it's not a file"))
        } else {
            Ok(cli)
        }
    }

    pub(crate) fn subscribe_addr(&self) -> SocketAddr {
        self.subscribe_addr
    }

    pub(crate) fn listen_addr(&self) -> SocketAddr {
        self.listen_addr
    }

    pub(crate) fn data_dir(&self) -> &PathBuf {
        &self.data_dir
    }
}

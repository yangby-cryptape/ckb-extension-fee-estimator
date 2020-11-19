use std::net::{SocketAddr, ToSocketAddrs as _};

use crate::error::{Error, Result};

pub(crate) struct Arguments {
    subscribe_addr: SocketAddr,
    listen_addr: SocketAddr,
    // TODO Persistence
    //persistent_dir: PathBuf,
}

impl Arguments {
    pub(crate) fn load() -> Result<Self> {
        let matches = clap::App::new("[Experimental] CKB Fee Estimator")
            .version(clap::crate_version!())
            .author(clap::crate_authors!("\n"))
            .arg(
                clap::Arg::with_name("subscribe-addr")
                    .long("subscribe-addr")
                    .help("The address of CKB RPC subscription.")
                    .takes_value(true),
            )
            .arg(
                clap::Arg::with_name("listen-addr")
                    .long("listen-addr")
                    .help("The HTTP address which CKB Fee Estimator RPC service will listen on.")
                    .takes_value(true),
            )
            /*
            .arg(
                clap::Arg::with_name("persistent-dir")
                    .long("persistent-dir")
                    .help("The directory where to store the caches.")
                    .takes_value(true),
            )
            */
            .get_matches();
        let subscribe_addr = matches
            .value_of("subscribe-addr")
            .ok_or_else(|| Error::config("subscribe-addr not found"))?
            .to_socket_addrs()
            .map_err(Error::config)?
            .next()
            .ok_or_else(|| Error::config("subscribe-addr is malformed"))?;
        let listen_addr = matches
            .value_of("listen-addr")
            .ok_or_else(|| Error::config("listen-addr not found"))?
            .to_socket_addrs()
            .map_err(Error::config)?
            .next()
            .ok_or_else(|| Error::config("listen-addr is malformed"))?;
        /*
        let persistent_dir = matches
            .value_of("persistent-dir")
            .ok_or_else(|| Error::config("persistent-dir not found"))
            .map(Path::new)
            .and_then(|p| {
                if p.exists() && !p.is_dir() {
                    Err(Error::config("persistent-dir exists but it's not a file"))
                } else {
                    Ok(p.to_path_buf())
                }
            })?;
        */
        Ok(Self {
            subscribe_addr,
            listen_addr,
            // persistent_dir,
        })
    }

    pub(crate) fn subscribe_addr(&self) -> SocketAddr {
        self.subscribe_addr
    }

    pub(crate) fn listen_addr(&self) -> SocketAddr {
        self.listen_addr
    }

    //pub(crate) fn persistent_dir(&self) -> &PathBuf {
    //    &self.persistent_dir
    //}
}

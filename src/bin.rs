extern crate time;

mod arguments;
mod error;
mod estimators;
mod patches;
mod prelude;
mod runtime;
mod server;
mod service;
mod shared;
mod statistics;
mod subscriber;
mod types;
mod utilities;
mod validator;

fn main() -> anyhow::Result<()> {
    env_logger::init();

    log::debug!("CKB Fee Estimator service is starting ...");

    let args = arguments::Cli::load()?;
    let service = service::Service::start(&args)?;
    service.wait()?;

    log::debug!("CKB Fee Estimator service has been shutdown.");
    Ok(())
}

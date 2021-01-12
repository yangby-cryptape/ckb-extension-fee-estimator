use std::sync::Arc;

use jsonrpc_http_server::Server;
use tokio::sync::Notify;

use crate::{
    arguments::Arguments, error::Result, estimators::FeeEstimatorController, runtime::Runtime,
    server, shared::Shared, statistics::Statistics, subscriber::Subscriber,
};

pub(crate) struct Service {
    _server: Server,
    _subscriber: Subscriber,
    runtime: Runtime,
}

impl Service {
    pub(crate) fn start(args: &Arguments) -> Result<Service> {
        let runtime = crate::runtime::initialize(args)?;
        let stats = Statistics::new(60 * 24 * 2);
        let estimators = FeeEstimatorController::initialize(&runtime, &stats);
        let shared = Shared::initialize(args, &runtime, &stats, estimators.clone())?;
        let _server = server::initialize(args.listen_addr(), estimators)?;
        let _subscriber = Subscriber::initialize(args.subscribe_addr(), shared)?;
        let service = Service {
            _server,
            _subscriber,
            runtime,
        };
        Ok(service)
    }

    pub(crate) fn wait(self) -> Result<()> {
        log::info!("service is blocking ...");

        {
            let notify = Arc::new(Notify::new());
            {
                let notify = Arc::clone(&notify);
                ctrlc::set_handler(move || {
                    log::trace!("capture the ctrl-c event");
                    notify.notify_one();
                })?;
            }
            log::debug!("service is waiting for ctrl-c ...");
            {
                self.runtime.block_on(async {
                    notify.notified().await;
                });
            }
        }

        log::info!("service is exiting ...");

        Ok(())
    }
}

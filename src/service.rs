use std::collections::HashMap;

use jsonrpc_http_server::Server;

use crate::{
    arguments::Arguments, error::Result, estimators, runtime, server, shared::Shared,
    statistics::Statistics, subscriber::Subscriber,
};

pub(crate) struct Service {
    server: Server,
    _subscriber: Subscriber,
}

impl Service {
    pub(crate) fn start(args: &Arguments) -> Result<Service> {
        let rt = runtime::initialize(args)?;
        let stats = Statistics::new(60 * 24 * 2);
        let estimators_map = {
            let mut estimators_map = HashMap::new();
            let controller =
                estimators::vbytes_flow::FeeEstimator::new(1_000, 60 * 24, &rt, &stats).spawn();
            estimators_map.insert("vbytes-flow".to_owned(), controller);
            estimators_map
        };
        let shared = Shared::initialize(args, &rt, &stats, estimators_map.clone())?;
        let server = server::initialize(args.listen_addr(), estimators_map)?;
        let _subscriber = Subscriber::initialize(args.subscribe_addr(), shared)?;
        let service = Service {
            server,
            _subscriber,
        };
        Ok(service)
    }

    pub(crate) fn wait(self) {
        log::info!("service is blocking");
        self.server.wait();
    }
}

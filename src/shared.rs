use std::{collections::HashMap, sync::Arc};

use parking_lot::RwLock;

use crate::{
    arguments::Arguments, error::Result, estimators::FeeEstimatorController, runtime::Runtime,
    statistics::Statistics, types,
};

#[derive(Clone)]
pub(crate) struct Shared {
    runtime: Runtime,
    statistics: Arc<RwLock<Statistics>>,
    estimators: HashMap<String, FeeEstimatorController>,
}

impl Shared {
    pub(crate) fn initialize(
        _args: &Arguments,
        rt: &Runtime,
        stats: &Arc<RwLock<Statistics>>,
        estimators: HashMap<String, FeeEstimatorController>,
    ) -> Result<Shared> {
        let runtime = rt.clone();
        let statistics = Arc::clone(&stats);
        Ok(Self {
            runtime,
            statistics,
            estimators,
        })
    }

    pub(crate) fn runtime(&self) -> Runtime {
        self.runtime.clone()
    }

    pub(crate) fn submit_transaction(&self, tx: types::Transaction) {
        for (name, estimator) in &self.estimators {
            log::trace!("submit transaction to {}", name);
            estimator.submit_transaction(tx.clone());
        }
        self.statistics.write().submit_transaction(&tx);
    }

    pub(crate) fn commit_block(&self, block: types::Block) {
        for (name, estimator) in &self.estimators {
            log::trace!("commit block to {}", name);
            estimator.commit_block(block.clone())
        }
        self.statistics.write().commit_block(&block);
    }
}

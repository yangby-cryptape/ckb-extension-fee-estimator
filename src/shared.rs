use std::sync::Arc;

use parking_lot::RwLock;

use crate::{
    arguments::Cli, error::Result, estimators::FeeEstimatorController, runtime::Runtime,
    statistics::Statistics, types,
};

#[derive(Clone)]
pub(crate) struct Shared {
    runtime: Runtime,
    statistics: Arc<RwLock<Statistics>>,
    estimators: FeeEstimatorController,
}

impl Shared {
    pub(crate) fn initialize(
        _cli: &Cli,
        rt: &Runtime,
        stats: &Arc<RwLock<Statistics>>,
        estimators: FeeEstimatorController,
    ) -> Result<Shared> {
        let runtime = rt.clone();
        let statistics = Arc::clone(stats);
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
        self.estimators.submit_transaction(tx.clone());
        self.statistics.write().submit_transaction(&tx);
    }

    pub(crate) fn commit_block(&self, block: types::Block) {
        self.estimators.commit_block(block.clone());
        self.statistics.write().commit_block(&block);
    }

    pub(crate) fn reject_transaction(&self, tx: types::RejectedTransaction) {
        self.estimators.reject_transaction(tx.clone());
        self.statistics.write().reject_transaction(&tx);
    }
}

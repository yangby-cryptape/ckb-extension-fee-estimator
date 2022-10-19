use std::{collections::HashMap, sync::Arc};

use parking_lot::RwLock;
use serde_json::Value;

use crate::{
    error::{RpcError, RpcResult},
    runtime::Runtime,
    statistics::Statistics,
    types,
};

mod algorithms;

use algorithms::{
    confirmation_fraction::FeeEstimator as ConfirmationFractionFE,
    vbytes_flow::FeeEstimator as VbytesFlowFE,
};

#[derive(Clone)]
pub(crate) struct FeeEstimatorController {
    estimators: Arc<HashMap<&'static str, algorithms::Controller>>,
}

impl FeeEstimatorController {
    pub(crate) fn initialize(rt: &Runtime, stats: &Arc<RwLock<Statistics>>) -> Self {
        let estimators = {
            let mut estimators = HashMap::new();
            let controller = VbytesFlowFE::new_controller(1_000, 60 * 24, rt, stats);
            estimators.insert(controller.name(), controller);
            let controller =
                ConfirmationFractionFE::new_controller(10_000, 20, 1e3, 1e7, 1.05, rt, stats);
            estimators.insert(controller.name(), controller);
            Arc::new(estimators)
        };
        Self { estimators }
    }

    pub(crate) fn estimate_fee_rate(
        &self,
        algorithm: &str,
        inputs: Value,
    ) -> RpcResult<Option<types::FeeRate>> {
        self.estimators
            .get(algorithm)
            .ok_or_else(|| RpcError::other(format!("no such algorithm `{}`", algorithm)))
            .and_then(|controller| controller.estimate_fee_rate(inputs))
    }

    pub(crate) fn submit_transaction(&self, tx: types::Transaction) {
        for (name, estimator) in self.estimators.iter() {
            log::trace!("submit transaction to {}", name);
            estimator.submit_transaction(tx.clone());
        }
    }

    pub(crate) fn commit_block(&self, block: types::Block) {
        for (name, estimator) in self.estimators.iter() {
            log::trace!("commit block to {}", name);
            estimator.commit_block(block.clone())
        }
    }

    pub(crate) fn reject_transaction(&self, tx: types::RejectedTransaction) {
        for (name, estimator) in self.estimators.iter() {
            log::trace!("reject transaction to {}", name);
            estimator.reject_transaction(tx.clone())
        }
    }
}

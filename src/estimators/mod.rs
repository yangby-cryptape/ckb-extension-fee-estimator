use std::fmt;

use serde_json::Value;
use tokio::sync::{mpsc, oneshot};

use crate::{
    error::{RpcError, RpcResult},
    runtime::Runtime,
    types,
};

pub(crate) mod vbytes_flow;

#[derive(Debug)]
enum FeeEstimatorParams {
    Estimate(Value),
    NewTransaction(types::Transaction),
    NewBlock(types::Block),
}

#[derive(Debug)]
enum FeeEstimatorResult {
    Estimate(RpcResult<Option<types::FeeRate>>),
    NoReturn,
}

#[derive(Clone)]
pub(crate) struct FeeEstimatorController {
    runtime: Runtime,
    sender: mpsc::Sender<(
        FeeEstimatorParams,
        Option<oneshot::Sender<FeeEstimatorResult>>,
    )>,
}

impl fmt::Display for FeeEstimatorParams {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Estimate(_) => "Estimate",
            Self::NewTransaction(_) => "NewTransaction",
            Self::NewBlock(_) => "NewBlock",
        };
        write!(f, "{}", name)
    }
}

impl FeeEstimatorController {
    pub(crate) fn estimate_fee_rate(&self, inputs: Value) -> RpcResult<Option<types::FeeRate>> {
        log::trace!("estimate fee rate");
        let mut sender = self.sender.clone();
        let (sender1, receiver1) = oneshot::channel();
        let inputs = FeeEstimatorParams::Estimate(inputs);
        if let Err(err) = sender.try_send((inputs, Some(sender1))) {
            log::error!("failed to send a message since {}", err);
        }
        let recv_data = self
            .runtime
            .block_on(receiver1)
            .map_err(|_| RpcError::other("internal error: broken channel"))?;
        if let FeeEstimatorResult::Estimate(res) = recv_data {
            res
        } else {
            Err(RpcError::other(
                "internal error: unexpected estimate result",
            ))
        }
    }

    pub(crate) fn submit_transaction(&self, tx: types::Transaction) {
        let mut sender = self.sender.clone();
        let inputs = FeeEstimatorParams::NewTransaction(tx);
        if let Err(err) = sender.try_send((inputs, None)) {
            log::error!("failed to send a message since {}", err);
        }
    }

    pub(crate) fn commit_block(&self, block: types::Block) {
        let mut sender = self.sender.clone();
        let inputs = FeeEstimatorParams::NewBlock(block);
        if let Err(err) = sender.try_send((inputs, None)) {
            log::error!("failed to send a message since {}", err);
        }
    }
}

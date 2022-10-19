use std::fmt;

use serde_json::Value;
use tokio::sync::{mpsc, oneshot};

use crate::{
    error::{RpcError, RpcResult},
    runtime::Runtime,
    types,
};

pub(super) mod confirmation_fraction;
pub(super) mod vbytes_flow;

#[derive(Debug)]
enum Params {
    Estimate(Value),
    NewTransaction(types::Transaction),
    NewBlock(types::Block),
}

#[derive(Debug)]
enum Result {
    Estimate(RpcResult<Option<types::FeeRate>>),
    NoReturn,
}

#[derive(Clone)]
pub(super) struct Controller {
    name: &'static str,
    runtime: Runtime,
    sender: mpsc::Sender<(Params, Option<oneshot::Sender<self::Result>>)>,
}

impl fmt::Display for Params {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let name = match self {
            Self::Estimate(_) => "Estimate",
            Self::NewTransaction(_) => "NewTransaction",
            Self::NewBlock(_) => "NewBlock",
        };
        write!(f, "{}", name)
    }
}

impl Controller {
    pub(super) fn name(&self) -> &'static str {
        self.name
    }

    pub(super) fn estimate_fee_rate(&self, inputs: Value) -> RpcResult<Option<types::FeeRate>> {
        log::trace!("estimate fee rate");
        let sender = self.sender.clone();
        let (sender1, receiver1) = oneshot::channel();
        let inputs = Params::Estimate(inputs);
        if let Err(err) = sender.try_send((inputs, Some(sender1))) {
            log::error!("failed to send a message since {}", err);
        }
        let recv_data = self
            .runtime
            .block_on(receiver1)
            .map_err(|_| RpcError::other("internal error: broken channel"))?;
        if let self::Result::Estimate(res) = recv_data {
            res
        } else {
            Err(RpcError::other(
                "internal error: unexpected estimate result",
            ))
        }
    }

    pub(super) fn submit_transaction(&self, tx: types::Transaction) {
        let sender = self.sender.clone();
        let inputs = Params::NewTransaction(tx);
        if let Err(err) = sender.try_send((inputs, None)) {
            log::error!("failed to send a message since {}", err);
        }
    }

    pub(super) fn commit_block(&self, block: types::Block) {
        let sender = self.sender.clone();
        let inputs = Params::NewBlock(block);
        if let Err(err) = sender.try_send((inputs, None)) {
            log::error!("failed to send a message since {}", err);
        }
    }
}

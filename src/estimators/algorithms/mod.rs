use std::{
    fmt,
    sync::mpsc::{sync_channel, SyncSender},
};

use serde_json::Value;
use tokio::{sync::mpsc::Sender, task::block_in_place};

use crate::{
    error::{RpcError, RpcResult},
    types,
};

pub(super) mod confirmation_fraction;
pub(super) mod vbytes_flow;

#[derive(Debug)]
enum Params {
    Estimate(Value),
    NewTransaction(types::Transaction),
    NewBlock(types::Block),
    RejectTransaction(types::RejectedTransaction),
}

#[derive(Debug)]
enum Result {
    Estimate(RpcResult<Option<types::FeeRate>>),
    NoReturn,
}

#[derive(Clone)]
pub(super) struct Controller {
    name: &'static str,
    sender: Sender<(Params, Option<SyncSender<self::Result>>)>,
}

impl fmt::Display for Params {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let name = match self {
            Self::Estimate(_) => "Estimate",
            Self::NewTransaction(_) => "NewTransaction",
            Self::NewBlock(_) => "NewBlock",
            Self::RejectTransaction(_) => "RejectTransaction",
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
        let (sender1, receiver1) = sync_channel(1);
        let inputs = Params::Estimate(inputs);
        if let Err(err) = sender.try_send((inputs, Some(sender1))) {
            log::error!("failed to send a message since {}", err);
        }
        let recv_data = block_in_place(|| receiver1.recv()).map_err(|err| {
            let errmsg = format!("internal error: broken channel since {}", err);
            RpcError::other(errmsg)
        })?;
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

    pub(crate) fn reject_transaction(&self, tx: types::RejectedTransaction) {
        let sender = self.sender.clone();
        let inputs = Params::RejectTransaction(tx);
        if let Err(err) = sender.try_send((inputs, None)) {
            log::error!("failed to send a message since {}", err);
        }
    }
}

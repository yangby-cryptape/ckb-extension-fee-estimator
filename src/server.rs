use std::{collections::HashMap, net::SocketAddr};

use jsonrpc_core::{IoHandler, Result as JsonRpcResult};
use jsonrpc_derive::rpc;
use jsonrpc_http_server::{Server, ServerBuilder};
use jsonrpc_server_utils::{cors::AccessControlAllowOrigin, hosts::DomainsValidation};
use serde::Deserialize;
use serde_json::Value;

use crate::{
    error::{Error, Result, RpcError},
    estimators::FeeEstimatorController,
    types::FeeRate,
};

pub(crate) struct FeeRateRpcImpl {
    estimators: HashMap<String, FeeEstimatorController>,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct EstimateParams {
    algorithm: String,
    #[serde(flatten)]
    inputs: Value,
}

#[rpc(server)]
pub trait FeeRateRpc {
    #[rpc(name = "estimate_fee_rate")]
    fn estimate_fee_rate(&self, params: EstimateParams) -> JsonRpcResult<Option<FeeRate>>;
}

impl FeeRateRpc for FeeRateRpcImpl {
    fn estimate_fee_rate(&self, params: EstimateParams) -> JsonRpcResult<Option<FeeRate>> {
        if let Some(controller) = self.estimators.get(&params.algorithm) {
            controller
                .estimate_fee_rate(params.inputs)
                .map_err(Into::into)
        } else {
            Err(RpcError::other(format!("no such algorithm `{}`", &params.algorithm)).into())
        }
    }
}

pub(crate) fn initialize(
    addr: SocketAddr,
    estimators: HashMap<String, FeeEstimatorController>,
) -> Result<Server> {
    log::trace!("initialize a HTTP JSON-RPC server ...");
    let mut io_handler = IoHandler::new();
    let rpc_impl = FeeRateRpcImpl { estimators };
    io_handler.extend_with(rpc_impl.to_delegate());
    ServerBuilder::new(io_handler)
        .cors(DomainsValidation::AllowOnly(vec![
            AccessControlAllowOrigin::Null,
            AccessControlAllowOrigin::Any,
        ]))
        .start_http(&addr)
        .map_err(Error::server)
}

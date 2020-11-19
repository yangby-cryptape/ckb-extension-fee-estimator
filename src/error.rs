use std::{fmt, result};

use jsonrpc_core as rpc;
use thiserror::Error;

#[derive(Error, Debug)]
pub(crate) enum Error {
    #[error("config error: {0}")]
    Config(String),
    #[error("runtime error: {0}")]
    Runtime(String),
    #[error("subscriber error: {0}")]
    Subscriber(String),
    #[error("server error: {0}")]
    Server(String),
    #[error("serialize error: {0}")]
    Serialize(#[from] ckb_types::error::VerificationError),
    #[error("channel error: {0}")]
    Receiver(#[from] tokio::sync::oneshot::error::RecvError),
}

#[derive(Error, Debug)]
pub(crate) enum RpcError {
    #[error("invalid params: {0}")]
    InvalidParams(String),
    #[error("parse error: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("error: {0}")]
    Other(String),
}

pub(crate) type Result<T> = result::Result<T, Error>;
pub(crate) type RpcResult<T> = result::Result<T, RpcError>;

impl Error {
    pub(crate) fn config<T: fmt::Display>(inner: T) -> Self {
        Self::Config(inner.to_string())
    }
    pub(crate) fn runtime<T: fmt::Display>(inner: T) -> Self {
        Self::Runtime(inner.to_string())
    }
    pub(crate) fn subscriber<T: fmt::Display>(inner: T) -> Self {
        Self::Subscriber(inner.to_string())
    }
    pub(crate) fn server<T: fmt::Display>(inner: T) -> Self {
        Self::Server(inner.to_string())
    }
}

impl RpcError {
    pub(crate) fn invalid_params<T: fmt::Display>(inner: T) -> Self {
        Self::InvalidParams(inner.to_string())
    }
    pub(crate) fn other<T: fmt::Display>(inner: T) -> Self {
        Self::Other(inner.to_string())
    }
}

impl From<RpcError> for rpc::Error {
    fn from(error: RpcError) -> Self {
        match error {
            RpcError::InvalidParams(message) => rpc::Error {
                code: rpc::ErrorCode::InvalidParams,
                message,
                data: None,
            },
            RpcError::Parse(err) => rpc::Error {
                code: rpc::ErrorCode::ParseError,
                message: err.to_string(),
                data: None,
            },
            RpcError::Other(message) => rpc::Error {
                code: rpc::ErrorCode::InternalError,
                message,
                data: None,
            },
        }
    }
}

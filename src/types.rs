use std::{result::Result as StdResult, str::FromStr, time::Duration};

use ckb_jsonrpc_types as rpc;
use ckb_types::{core, packed, prelude::*};

use crate::utilities::unix_timestamp;

pub(crate) type FeeRate = u64;

#[derive(Debug, Clone)]
pub(crate) struct Block {
    hash: packed::Byte32,
    number: u64,
    timestamp: u64,
    transactions: Vec<packed::Byte32>,
    seen_dt: Duration,
}

#[derive(Debug, Clone)]
pub(crate) struct Transaction {
    hash: packed::Byte32,
    cycles: u64,
    size: u64,
    fee: u64,
    seen_dt: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RejectedType {
    Invalid,
    Exceeded,
}

#[derive(Debug, Clone)]
pub(crate) struct RejectedTransaction {
    transaction: Transaction,
    rejected_type: RejectedType,
    rejected_desc: String,
}

impl From<rpc::BlockView> for Block {
    fn from(json: rpc::BlockView) -> Self {
        let block: core::BlockView = json.into();
        let hash = block.hash();
        let number = block.number();
        let timestamp = block.timestamp();
        let transactions: Vec<packed::Byte32> = block.tx_hashes().to_owned();
        let seen_dt = unix_timestamp();
        Self {
            hash,
            number,
            timestamp,
            transactions,
            seen_dt,
        }
    }
}

impl FromStr for Block {
    type Err = serde_json::Error;
    fn from_str(s: &str) -> StdResult<Self, Self::Err> {
        serde_json::from_str::<rpc::BlockView>(s).map(From::from)
    }
}

impl Block {
    pub(crate) fn hash(&self) -> packed::Byte32 {
        self.hash.clone()
    }

    pub(crate) fn number(&self) -> u64 {
        self.number
    }

    pub(crate) fn timestamp(&self) -> u64 {
        self.timestamp
    }

    pub(crate) fn tx_hashes(&self) -> &[packed::Byte32] {
        &self.transactions[..]
    }

    pub(crate) fn seen_dt(&self) -> Duration {
        self.seen_dt
    }
}

impl From<rpc::PoolTransactionEntry> for Transaction {
    fn from(json: rpc::PoolTransactionEntry) -> Self {
        let hash: packed::Byte32 = json.transaction.hash.pack();
        let cycles: u64 = json.cycles.into();
        let size: u64 = json.size.into();
        let fee: u64 = json.fee.into();
        let seen_dt = unix_timestamp();
        Self {
            hash,
            cycles,
            size,
            fee,
            seen_dt,
        }
    }
}

impl FromStr for Transaction {
    type Err = serde_json::Error;
    fn from_str(s: &str) -> StdResult<Self, Self::Err> {
        serde_json::from_str::<rpc::PoolTransactionEntry>(s).map(From::from)
    }
}

impl Transaction {
    pub(crate) fn hash(&self) -> packed::Byte32 {
        self.hash.clone()
    }

    pub(crate) fn cycles(&self) -> u64 {
        self.cycles
    }

    pub(crate) fn size(&self) -> u64 {
        self.size
    }

    pub(crate) fn fee(&self) -> u64 {
        self.fee
    }

    pub(crate) fn seen_dt(&self) -> Duration {
        self.seen_dt
    }
}

impl From<(rpc::PoolTransactionEntry, rpc::PoolTransactionReject)> for RejectedTransaction {
    fn from(json: (rpc::PoolTransactionEntry, rpc::PoolTransactionReject)) -> Self {
        use ckb_jsonrpc_types::PoolTransactionReject as Reject;
        let transaction = json.0.into();
        let (rejected_type, rejected_desc) = match json.1 {
            Reject::LowFeeRate(desc) => (RejectedType::Exceeded, desc),
            Reject::ExceededMaximumAncestorsCount(desc) => (RejectedType::Invalid, desc),
            Reject::Full(desc) => (RejectedType::Exceeded, desc),
            Reject::Duplicated(desc) => (RejectedType::Exceeded, desc),
            Reject::Malformed(desc) => (RejectedType::Invalid, desc),
            Reject::DeclaredWrongCycles(desc) => (RejectedType::Invalid, desc),
            Reject::Resolve(desc) => (RejectedType::Invalid, desc),
            Reject::Verification(desc) => (RejectedType::Invalid, desc),
            Reject::Expiry(desc) => (RejectedType::Exceeded, desc),
        };
        Self {
            transaction,
            rejected_type,
            rejected_desc,
        }
    }
}

impl FromStr for RejectedTransaction {
    type Err = serde_json::Error;
    fn from_str(s: &str) -> StdResult<Self, Self::Err> {
        serde_json::from_str::<(rpc::PoolTransactionEntry, rpc::PoolTransactionReject)>(s)
            .map(From::from)
    }
}

impl RejectedTransaction {
    pub(crate) fn transaction(&self) -> &Transaction {
        &self.transaction
    }

    pub(crate) fn is_invalid(&self) -> bool {
        self.rejected_type == RejectedType::Invalid
    }

    pub(crate) fn reason(&self) -> &str {
        &self.rejected_desc
    }

    pub(crate) fn hash(&self) -> packed::Byte32 {
        self.transaction().hash()
    }
}

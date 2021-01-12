use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
    time::Duration,
};

use ckb_fee_estimator::FeeRate;
use ckb_types::{core::Capacity, packed};
use parking_lot::RwLock;
use serde::Deserialize;
use statrs::distribution::{Poisson, Univariate};
use tokio::{
    select,
    sync::{mpsc, oneshot},
};

use crate::{
    error::{RpcError, RpcResult},
    patches,
    prelude::*,
    runtime::Runtime,
    statistics::Statistics,
    types,
    utilities::unix_timestamp,
};

const NAME: &str = "vbytes-flow";

const FEE_RATE_UNIT: u64 = 1000;

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
struct Params {
    probability: f32,
    target_minutes: u32,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct TxStatus {
    pub(crate) vbytes: usize,
    pub(crate) fee_rate: FeeRate,
}

#[derive(Debug, Clone)]
pub(crate) struct TxAdded {
    pub(crate) status: TxStatus,
    pub(crate) added_dt: Duration,
}

struct PendingRecord {
    added_dt: Duration,
    expected_dt: Duration,
}

struct TxAddedQue(VecDeque<TxAdded>);

struct TestSet {
    pending: HashMap<packed::Byte32, PendingRecord>,
    failure: Vec<Duration>,
    success: Vec<Duration>,
}

/// Weight-Units Flow Fee Estimator
///
/// Ref: https://bitcoiner.live/?tab=info
pub(in crate::estimators) struct FeeEstimator {
    lowest_fee_rate: FeeRate,
    max_target_dur: Duration,
    boot_dt: Duration,
    txs: TxAddedQue,
    testset: TestSet,
    statistics: Arc<RwLock<Statistics>>,
}

impl TxStatus {
    pub(crate) fn new(vbytes: usize, fee_rate: FeeRate) -> Self {
        Self { vbytes, fee_rate }
    }
}

impl PartialOrd for TxStatus {
    fn partial_cmp(&self, other: &TxStatus) -> Option<::std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for TxStatus {
    fn cmp(&self, other: &Self) -> ::std::cmp::Ordering {
        let order = self.fee_rate.cmp(&other.fee_rate);
        if order == ::std::cmp::Ordering::Equal {
            other.vbytes.cmp(&self.vbytes)
        } else {
            order
        }
    }
}

impl TxAdded {
    pub(crate) fn new(vbytes: usize, fee_rate: FeeRate, added_dt: Duration) -> Self {
        Self {
            status: TxStatus::new(vbytes, fee_rate),
            added_dt,
        }
    }
}

impl TxAddedQue {
    fn add_transaction(&mut self, tx: TxAdded) {
        self.0.push_front(tx);
    }

    fn expire(&mut self, expired_dt: Duration) -> usize {
        let count = self
            .0
            .iter()
            .rev()
            .skip_while(|tx| tx.added_dt < expired_dt)
            .count();
        let total = self.0.len();
        if count > 0 && total >= count {
            self.0.truncate(total - count);
        }
        count
    }

    fn flowed(&self, historical_dt: Duration) -> Vec<TxStatus> {
        self.0
            .iter()
            .rev()
            .skip_while(|tx| tx.added_dt >= historical_dt)
            .map(|tx| &tx.status)
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>()
    }
}

impl TestSet {
    fn expire(&mut self, current_dt: Duration) {
        let mut failure = Vec::new();
        for hash in self.pending.keys() {
            let expected_dt = self.pending.get(&hash).unwrap().expected_dt;
            if current_dt > expected_dt {
                failure.push(hash.to_owned());
            }
        }
        for hash in failure {
            let tx = self.pending.remove(&hash).unwrap();
            self.failure.push(tx.added_dt);
        }
    }

    fn confirm(&mut self, block: &types::Block) {
        let mut success = Vec::new();
        for hash in block.tx_hashes() {
            if self.pending.contains_key(&hash) {
                success.push(hash.to_owned());
            }
        }
        for hash in success {
            let tx = self.pending.remove(&hash).unwrap();
            self.success.push(tx.added_dt);
        }
    }
    fn score(&self) -> (usize, usize) {
        (self.success.len(), self.failure.len())
    }
}

impl FeeEstimator {
    fn historical_dur(target_dur: Duration) -> Duration {
        if target_dur <= Duration::from_secs(5 * 60) {
            Duration::from_secs(10 * 60)
        } else {
            target_dur * 2
        }
    }

    fn do_estimate(&mut self, params: &Params) -> Option<u64> {
        log::trace!(
            "probability: {:.2}%, target: {} mins",
            params.probability * 100.0,
            params.target_minutes
        );
        let current_dt = unix_timestamp();
        let target_dur = Duration::from_secs(u64::from(params.target_minutes) * 60);
        let historical_dur = Self::historical_dur(target_dur);
        log::trace!(
            "current timestamp: {}, target: {:?}, historical: {:?}",
            current_dt.pretty(),
            target_dur,
            historical_dur,
        );
        let average_blocks = {
            let filter_blocks_dt = current_dt - historical_dur;
            let blocks_count = self
                .statistics
                .read()
                .filter_blocks(|dt| dt >= filter_blocks_dt, |_, _| Some(()))
                .len() as u32;
            if blocks_count == 0 {
                log::warn!("historical: no blocks");
                return None;
            }
            let interval_dur = historical_dur / blocks_count;
            log::trace!(
                "historical: blocks count: {} in {:?} (interval: {:?})",
                blocks_count,
                historical_dur,
                interval_dur
            );
            let average_blocks = (target_dur.as_millis() / interval_dur.as_millis()) as u32;
            log::trace!(
                "average: blocks count: {} in {:?}",
                average_blocks,
                target_dur,
            );
            average_blocks
        };
        if average_blocks == 0 {
            return None;
        }
        let current_txs = self.statistics.read().filter_transactions(
            |_| true,
            |_, tx| {
                let vbytes = patches::get_transaction_virtual_bytes(tx.size() as usize, tx.cycles())
                    as usize;
                let fee_rate = FeeRate::calculate(Capacity::shannons(tx.fee()), vbytes);
                let tx_status = TxStatus::new(vbytes, fee_rate);
                Some(tx_status)
            },
        );
        log::trace!("current transactions count = {}", current_txs.len());
        self.do_estimate_internal(
            params.probability,
            target_dur,
            historical_dur,
            current_dt,
            current_txs,
            patches::MAX_BLOCK_WEIGHT,
            average_blocks,
        )
        .map(FeeRate::as_u64)
    }

    #[allow(clippy::too_many_arguments)]
    fn do_estimate_internal(
        &mut self,
        probability: f32,
        target_dur: Duration,
        historical_dur: Duration,
        current_dt: Duration,
        mut current_txs: Vec<TxStatus>,
        max_block_weight: u64,
        average_blocks: u32,
    ) -> Option<FeeRate> {
        current_txs.sort();
        current_txs.reverse();
        if let Some(max_fee_rate) = current_txs.first().map(|tx| tx.fee_rate) {
            log::trace!("max fee rate of current transactions: {}", max_fee_rate);
            let max_bucket_index = Self::max_bucket_index_by_fee_rate(max_fee_rate);
            log::trace!("current weight buckets size: {}", max_bucket_index + 1);
            let current_weight_buckets = {
                let mut current_weight = vec![0u64; max_bucket_index + 1];
                let mut index_curr = max_bucket_index;
                for tx in &current_txs {
                    let index = Self::max_bucket_index_by_fee_rate(tx.fee_rate);
                    if index < index_curr {
                        let weight_curr = current_weight[index_curr];
                        for i in current_weight.iter_mut().take(index_curr).skip(index) {
                            *i = weight_curr;
                        }
                    }
                    index_curr = index;
                    current_weight[index] += tx.vbytes as u64;
                }
                current_weight
            };
            for (index, bucket) in current_weight_buckets.iter().enumerate() {
                if *bucket != 0 {
                    log::trace!(">>> current_weight[{}]: {}", index, bucket);
                }
            }
            let flow_speed_buckets = {
                let historical_dt = current_dt - historical_dur;
                let mut txs_flowed = self.txs.flowed(historical_dt);
                txs_flowed.sort();
                txs_flowed.reverse();
                let mut flowed = vec![0u64; max_bucket_index + 1];
                let mut index_curr = max_bucket_index;
                for tx in &txs_flowed {
                    let index = Self::max_bucket_index_by_fee_rate(tx.fee_rate);
                    if index < index_curr {
                        let flowed_curr = flowed[index_curr];
                        for i in flowed.iter_mut().take(index_curr).skip(index) {
                            *i = flowed_curr;
                        }
                    }
                    index_curr = index;
                    flowed[index] += tx.vbytes as u64;
                }
                flowed
                    .into_iter()
                    .map(|value| value / historical_dur.as_secs())
                    .collect::<Vec<_>>()
            };
            for (index, bucket) in flow_speed_buckets.iter().enumerate() {
                if *bucket != 0 {
                    log::trace!(">>> flow_speed[{}]: {}", index, bucket);
                }
            }
            let expected_blocks = {
                let mut blocks = 0u32;
                let poisson = Poisson::new(f64::from(average_blocks)).unwrap();
                loop {
                    let expected_probability = 1.0 - poisson.cdf(f64::from(blocks));
                    if expected_probability < f64::from(probability) {
                        break;
                    }
                    blocks += 1;
                }
                u64::from(blocks)
            };
            log::trace!("expected block count: {}", expected_blocks);
            for bucket_index in 1..(max_bucket_index + 1) {
                let current_weight = current_weight_buckets[bucket_index];
                let added_weight = flow_speed_buckets[bucket_index] * target_dur.as_secs();
                let removed_weight = max_block_weight * expected_blocks;
                let passed = current_weight + added_weight <= removed_weight;
                log::trace!(
                    ">>> bucket[{}]: {}; {} + {} - {}",
                    bucket_index,
                    passed,
                    current_weight,
                    added_weight,
                    removed_weight
                );
                if passed {
                    return Some(Self::lowest_fee_rate_by_bucket_index(bucket_index));
                }
            }
            None
        } else {
            Some(self.lowest_fee_rate)
        }
    }
}

impl FeeEstimator {
    fn lowest_fee_rate_by_bucket_index(index: usize) -> FeeRate {
        let t = FEE_RATE_UNIT;
        let value = match index as u64 {
            // 0->0
            0 => 0,
            // 1->1000, 2->2000, .., 10->10000
            x if x <= 10 => t * x,
            // 11->12000, 12->14000, .., 30->50000
            x if x <= 30 => t * (10 + (x - 10) * 2),
            // 31->55000, 32->60000, ..., 60->200000
            x if x <= 60 => t * (10 + 20 * 2 + (x - 30) * 5),
            // 61->210000, 62->220000, ..., 90->500000
            x if x <= 90 => t * (10 + 20 * 2 + 30 * 5 + (x - 60) * 10),
            // 91->520000, 92->540000, ..., 115 -> 1000000
            x if x <= 115 => t * (10 + 20 * 2 + 30 * 5 + 30 * 10 + (x - 90) * 20),
            // 116->1050000, 117->1100000, ..., 135->2000000
            x if x <= 135 => t * (10 + 20 * 2 + 30 * 5 + 30 * 10 + 25 * 20 + (x - 115) * 50),
            // 136->2100000,  137->2200000, ...
            x => t * (10 + 20 * 2 + 30 * 5 + 30 * 10 + 25 * 20 + 20 * 50 + (x - 135) * 100),
        };
        FeeRate::from_u64(value)
    }

    fn max_bucket_index_by_fee_rate(fee_rate: FeeRate) -> usize {
        let t = FEE_RATE_UNIT;
        let index = match fee_rate.as_u64() {
            x if x <= 10_000 => x / t,
            x if x <= 50_000 => (x + t * 10) / (2 * t),
            x if x <= 200_000 => (x + t * 100) / (5 * t),
            x if x <= 500_000 => (x + t * 400) / (10 * t),
            x if x <= 1_000_000 => (x + t * 1_300) / (20 * t),
            x if x <= 2_000_000 => (x + t * 4_750) / (50 * t),
            x => (x + t * 11_500) / (100 * t),
        };
        index as usize
    }
}

#[cfg(test)]
mod tests {
    use super::FeeEstimator;
    use ckb_fee_estimator::FeeRate;

    #[test]
    fn test_bucket_index_and_fee_rate_expected() {
        let testdata = [
            (0, 0),
            (1, 1_000),
            (2, 2_000),
            (10, 10_000),
            (11, 12_000),
            (12, 14_000),
            (30, 50_000),
            (31, 55_000),
            (32, 60_000),
            (60, 200_000),
            (61, 210_000),
            (62, 220_000),
            (90, 500_000),
            (91, 520_000),
            (92, 540_000),
            (115, 1_000_000),
            (116, 1_050_000),
            (117, 1_100_000),
            (135, 2_000_000),
            (136, 2_100_000),
            (137, 2_200_000),
        ];
        for (bucket_index, fee_rate) in &testdata[..] {
            let expected_fee_rate =
                FeeEstimator::lowest_fee_rate_by_bucket_index(*bucket_index).as_u64();
            assert_eq!(expected_fee_rate, *fee_rate);
            let actual_bucket_index =
                FeeEstimator::max_bucket_index_by_fee_rate(FeeRate::from_u64(*fee_rate));
            assert_eq!(actual_bucket_index, *bucket_index);
        }
    }

    #[test]
    fn test_bucket_index_and_fee_rate_continuous() {
        for fee_rate in 0..3_000_000 {
            let bucket_index =
                FeeEstimator::max_bucket_index_by_fee_rate(FeeRate::from_u64(fee_rate));
            let fee_rate_le = FeeEstimator::lowest_fee_rate_by_bucket_index(bucket_index).as_u64();
            let fee_rate_gt =
                FeeEstimator::lowest_fee_rate_by_bucket_index(bucket_index + 1).as_u64();
            assert!(
                fee_rate_le <= fee_rate && fee_rate < fee_rate_gt,
                "Error for bucket[{}]: {} <= {} < {}",
                bucket_index,
                fee_rate_le,
                fee_rate,
                fee_rate_gt,
            );
        }
    }
}

impl FeeEstimator {
    pub(in crate::estimators) fn new_controller(
        lowest_fee_rate: u64,
        max_target_minutes: u64,
        rt: &Runtime,
        stats: &Arc<RwLock<Statistics>>,
    ) -> super::Controller {
        let testset = TestSet {
            pending: HashMap::new(),
            failure: Vec::new(),
            success: Vec::new(),
        };
        Self {
            lowest_fee_rate: FeeRate::from_u64(lowest_fee_rate),
            max_target_dur: Duration::from_secs(max_target_minutes * 60),
            boot_dt: unix_timestamp(),
            txs: TxAddedQue(VecDeque::new()),
            testset,
            statistics: Arc::clone(stats),
        }
        .spawn(rt)
    }

    fn spawn(self, rt: &Runtime) -> super::Controller {
        let (sender, mut receiver) =
            mpsc::channel::<(super::Params, Option<oneshot::Sender<super::Result>>)>(100);
        let runtime = rt.clone();
        runtime.spawn(async move {
            let mut estimator = self;
            loop {
                select! {
                    Some((message, sender1_opt)) = receiver.recv() => {
                        let resp = estimator.process(message).await;
                        if let Some(sender1) = sender1_opt {
                            let _ = sender1.send(resp);
                        }
                    },
                    else => break,
                }
            }
        });
        super::Controller {
            name: NAME,
            runtime,
            sender,
        }
    }

    async fn process(&mut self, msg: super::Params) -> super::Result {
        log::trace!("process {} message", msg);
        match msg {
            super::Params::Estimate(value) => match serde_json::from_value(value) {
                Ok(params) => {
                    if let Err(err) = self.check_estimate_params(&params) {
                        super::Result::Estimate(Err(err))
                    } else {
                        let fee_rate_opt = self.estimate(&params);
                        super::Result::Estimate(Ok(fee_rate_opt))
                    }
                }
                Err(err) => super::Result::Estimate(Err(err.into())),
            },
            super::Params::NewTransaction(tx) => {
                self.submit_transaction(&tx);
                super::Result::NoReturn
            }
            super::Params::NewBlock(block) => {
                self.commit_block(&block);
                super::Result::NoReturn
            }
        }
    }
}

impl FeeEstimator {
    fn can_estimate(&self, target_minutes: u32) -> bool {
        let target_dur = Duration::from_secs(u64::from(target_minutes) * 60);
        if target_dur > self.max_target_dur {
            false
        } else {
            let current_dt = unix_timestamp();
            let historical_dur = Self::historical_dur(target_dur);
            let required_boot_dt = current_dt - historical_dur;
            required_boot_dt > self.boot_dt
        }
    }

    fn check_estimate_params(&self, params: &Params) -> RpcResult<()> {
        if params.probability < 0.000_001 {
            return Err(RpcError::invalid_params(
                "probability should not less than 0.000_001",
            ));
        }
        if params.probability > 0.999_999 {
            return Err(RpcError::invalid_params(
                "probability should not greater than 0.999_999",
            ));
        }
        if params.target_minutes < 1 {
            return Err(RpcError::invalid_params(
                "target elapsed should not less than 1 minute",
            ));
        }
        if !self.can_estimate(params.target_minutes) {
            return Err(RpcError::other("lack of empirical data"));
        }
        Ok(())
    }

    fn estimate(&mut self, params: &Params) -> Option<u64> {
        self.do_estimate(params)
    }
}

impl FeeEstimator {
    fn submit_transaction(&mut self, tx: &types::Transaction) {
        let current_dt = tx.seen_dt();
        let expired_dt = current_dt - Self::historical_dur(self.max_target_dur);
        let vbytes = patches::get_transaction_virtual_bytes(tx.size() as usize, tx.cycles());
        let fee_rate = FeeRate::calculate(Capacity::shannons(tx.fee()), vbytes as usize);
        let new_tx = TxAdded::new(vbytes as usize, fee_rate, current_dt);
        self.txs.add_transaction(new_tx);
        self.txs.expire(expired_dt);
        {
            let mut minutes_opt: Option<u32> = None;
            let probability = 0.9;
            for target_minutes in &[10, 30, 60, 60 * 2, 60 * 3, 60 * 6, 60 * 12, 60 * 24] {
                if self.can_estimate(*target_minutes) {
                    let params = Params {
                        probability,
                        target_minutes: *target_minutes,
                    };
                    let result = self.estimate(&params);
                    if let Some(fee_rate_tmp) = result {
                        if fee_rate >= FeeRate::from_u64(fee_rate_tmp) {
                            minutes_opt = Some(*target_minutes);
                            break;
                        }
                    }
                } else {
                    log::trace!("new-tx: can not estimate");
                    break;
                }
            }
            if let Some(minutes) = minutes_opt {
                log::trace!(
                    "new-tx: tx {:#x} has {:.2}% probability commit in {} minutes",
                    tx.hash(),
                    probability * 100.0,
                    minutes
                );
                let added_dt = current_dt;
                let expected_dt = current_dt + Duration::from_secs(u64::from(minutes) * 60);
                let record = PendingRecord {
                    added_dt,
                    expected_dt,
                };
                self.testset.pending.insert(tx.hash(), record);
            } else {
                log::trace!("new-tx: no suitable fee rate");
            }
        }
    }

    fn commit_block(&mut self, block: &types::Block) {
        let current_dt = block.seen_dt();
        self.testset.expire(current_dt);
        self.testset.confirm(block);
        let (success_cnt, failure_cnt) = self.testset.score();
        let total = success_cnt + failure_cnt;
        let accuracy = f64::from(success_cnt as u32) / f64::from(total as u32);
        log::trace!("accuracy: {:.2} (total: {})", accuracy, total);
    }
}

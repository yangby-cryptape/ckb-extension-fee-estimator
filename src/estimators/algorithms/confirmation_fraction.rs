use std::{
    cmp,
    collections::{BTreeMap, HashMap},
    sync::{mpsc::SyncSender, Arc},
};

use ckb_types::{
    core::{Capacity, FeeRate},
    packed::Byte32,
};
use parking_lot::RwLock;
use serde::Deserialize;
use tokio::{select, sync::mpsc};

use crate::{
    error::{RpcError, RpcResult},
    patches,
    runtime::Runtime,
    statistics::Statistics,
    types,
    validator::Validator,
};

const NAME: &str = "confirmation-fraction";

#[derive(Default, Debug, Clone)]
struct BucketStat {
    total_fee_rate: FeeRate,
    txs_count: f64,
    old_unconfirmed_txs: usize,
}

/// TxConfirmStat is a struct to help to estimate txs fee rate,
/// This struct record txs fee_rate and blocks that txs to be committed.
///
/// We start from track unconfirmed txs,
/// When tx added to txpool, we increase the count of unconfirmed tx, we do opposite tx removed.
/// When a tx get committed, put it into bucket by tx fee_rate and confirmed blocks,
/// then decrease the count of unconfirmed txs.
///
/// So we get a group of samples which includes txs count, average fee rate and confirmed blocks, etc.
/// For estimate, we loop through each bucket, calculate the confirmed txs rate, until meet the required_confirm_rate.
#[derive(Clone)]
struct TxConfirmStat {
    min_fee_rate: FeeRate,
    /// per bucket stat
    bucket_stats: Vec<BucketStat>,
    /// bucket upper bound fee_rate => bucket index
    fee_rate_to_bucket: BTreeMap<FeeRate, usize>,
    /// confirm_blocks => bucket index => confirmed txs count
    confirm_blocks_to_confirmed_txs: Vec<Vec<f64>>,
    /// confirm_blocks => bucket index => failed txs count
    confirm_blocks_to_failed_txs: Vec<Vec<f64>>,
    /// Track recent N blocks unconfirmed txs
    /// tracked block index => bucket index => TxTracker
    block_unconfirmed_txs: Vec<Vec<usize>>,
    decay_factor: f64,
}

#[derive(Clone)]
struct TxRecord {
    height: u64,
    bucket_index: usize,
    fee_rate: FeeRate,
}

/// Fee Estimator
/// Estimator track new block and tx_pool to collect data
/// we track every new tx enter txpool and record the tip height and fee_rate,
/// when tx is packed into a new block or dropped by txpool,
/// we get a sample about how long a tx with X fee_rate can get confirmed or get dropped.
///
/// In inner, we group samples by predefined fee_rate buckets.
/// To estimator fee_rate for a confirm target(how many blocks that a tx can get committed),
/// we travel through fee_rate buckets, try to find a fee_rate X to let a tx get committed
/// with high probilities within confirm target blocks.
///
#[derive(Clone)]
struct Estimator {
    best_height: u64,
    start_height: u64,
    /// a data struct to track tx confirm status
    tx_confirm_stat: TxConfirmStat,
    tracked_txs: HashMap<Byte32, TxRecord>,
}

impl BucketStat {
    // add a new fee rate to this bucket
    fn new_fee_rate_sample(&mut self, fee_rate: FeeRate) {
        self.txs_count += 1f64;
        let total_fee_rate = self
            .total_fee_rate
            .as_u64()
            .saturating_add(fee_rate.as_u64());
        self.total_fee_rate = FeeRate::from_u64(total_fee_rate);
    }

    // get average fee rate from a bucket
    fn avg_fee_rate(&self) -> Option<FeeRate> {
        if self.txs_count > 0f64 {
            Some(FeeRate::from_u64(
                ((self.total_fee_rate.as_u64() as f64) / self.txs_count) as u64,
            ))
        } else {
            None
        }
    }
}

impl TxConfirmStat {
    fn new(buckets: &[FeeRate], max_confirm_blocks: usize, decay_factor: f64) -> Self {
        // max_confirm_blocsk: The number of blocks that the esitmator will trace the statistics.
        let min_fee_rate = buckets[0];
        let bucket_stats = vec![BucketStat::default(); buckets.len()];
        let confirm_blocks_to_confirmed_txs = vec![vec![0f64; buckets.len()]; max_confirm_blocks];
        let confirm_blocks_to_failed_txs = vec![vec![0f64; buckets.len()]; max_confirm_blocks];
        let block_unconfirmed_txs = vec![vec![0; buckets.len()]; max_confirm_blocks];
        let fee_rate_to_bucket = buckets
            .iter()
            .enumerate()
            .map(|(i, fee_rate)| (*fee_rate, i))
            .collect();
        TxConfirmStat {
            min_fee_rate,
            bucket_stats,
            fee_rate_to_bucket,
            block_unconfirmed_txs,
            confirm_blocks_to_confirmed_txs,
            confirm_blocks_to_failed_txs,
            decay_factor,
        }
    }

    /// Return upper bound fee_rate bucket
    /// assume we have three buckets with fee_rate [1.0, 2.0, 3.0], we return index 1 for fee_rate 1.5
    fn bucket_index_by_fee_rate(&self, fee_rate: FeeRate) -> Option<usize> {
        self.fee_rate_to_bucket
            .range(fee_rate..)
            .next()
            .map(|(_fee_rate, index)| *index)
    }

    fn max_confirms(&self) -> usize {
        self.confirm_blocks_to_confirmed_txs.len()
    }

    // add confirmed sample
    fn add_confirmed_tx(&mut self, blocks_to_confirm: usize, fee_rate: FeeRate) {
        if blocks_to_confirm < 1 {
            return;
        }
        let bucket_index = match self.bucket_index_by_fee_rate(fee_rate) {
            Some(index) => index,
            None => return,
        };
        // increase txs_count in buckets
        for i in (blocks_to_confirm - 1)..self.max_confirms() {
            self.confirm_blocks_to_confirmed_txs[i][bucket_index] += 1f64;
        }
        let stat = &mut self.bucket_stats[bucket_index];
        stat.new_fee_rate_sample(fee_rate);
    }

    // track an unconfirmed tx
    // entry_height - tip number when tx enter txpool
    fn add_unconfirmed_tx(&mut self, entry_height: u64, fee_rate: FeeRate) -> Option<usize> {
        let bucket_index = match self.bucket_index_by_fee_rate(fee_rate) {
            Some(index) => index,
            None => return None,
        };
        let block_index = (entry_height % (self.block_unconfirmed_txs.len() as u64)) as usize;
        self.block_unconfirmed_txs[block_index][bucket_index] += 1;
        Some(bucket_index)
    }

    fn remove_unconfirmed_tx(
        &mut self,
        entry_height: u64,
        tip_height: u64,
        bucket_index: usize,
        count_failure: bool,
    ) {
        let tx_age = tip_height.saturating_sub(entry_height) as usize;
        if tx_age < 1 {
            return;
        }
        if tx_age >= self.block_unconfirmed_txs.len() {
            self.bucket_stats[bucket_index].old_unconfirmed_txs -= 1;
        } else {
            let block_index = (entry_height % self.block_unconfirmed_txs.len() as u64) as usize;
            self.block_unconfirmed_txs[block_index][bucket_index] -= 1;
        }
        if count_failure {
            self.confirm_blocks_to_failed_txs[tx_age - 1][bucket_index] += 1f64;
        }
    }

    fn move_track_window(&mut self, height: u64) {
        let block_index = (height % (self.block_unconfirmed_txs.len() as u64)) as usize;
        for bucket_index in 0..self.bucket_stats.len() {
            // mark unconfirmed txs as old_unconfirmed_txs
            self.bucket_stats[bucket_index].old_unconfirmed_txs +=
                self.block_unconfirmed_txs[block_index][bucket_index];
            self.block_unconfirmed_txs[block_index][bucket_index] = 0;
        }
    }

    /// apply decay factor on stats, smoothly reduce the effects of old samples.
    fn decay(&mut self) {
        let decay_factor = self.decay_factor;
        for (bucket_index, bucket) in self.bucket_stats.iter_mut().enumerate() {
            self.confirm_blocks_to_confirmed_txs
                .iter_mut()
                .for_each(|buckets| {
                    buckets[bucket_index] *= decay_factor;
                });

            self.confirm_blocks_to_failed_txs
                .iter_mut()
                .for_each(|buckets| {
                    buckets[bucket_index] *= decay_factor;
                });
            bucket.total_fee_rate =
                FeeRate::from_u64((bucket.total_fee_rate.as_u64() as f64 * decay_factor) as u64);
            bucket.txs_count *= decay_factor;
            // TODO do we need decay the old unconfirmed?
        }
    }

    /// The naive estimate implementation
    /// 1. find best range of buckets satisfy the given condition
    /// 2. get median fee_rate from best range bucekts
    fn estimate_median(
        &self,
        confirm_blocks: usize,
        required_samples: usize,
        required_confirm_rate: f64,
    ) -> Option<FeeRate> {
        // A tx need 1 block to propose, then 2 block to get confirmed
        // so at least confirm blocks is 3 blocks.
        if confirm_blocks < 3 || required_samples == 0 {
            log::debug!(
                "confirm_blocks(={}) < 3 || required_samples(={}) == 0",
                confirm_blocks,
                required_samples
            );
            return None;
        }
        let mut confirmed_txs = 0f64;
        let mut txs_count = 0f64;
        let mut failure_count = 0f64;
        let mut extra_count = 0;
        let mut best_bucket_start = 0;
        let mut best_bucket_end = 0;
        let mut start_bucket_index = 0;
        let mut find_best = false;
        // try find enough sample data from buckets
        for (bucket_index, stat) in self.bucket_stats.iter().enumerate() {
            confirmed_txs += self.confirm_blocks_to_confirmed_txs[confirm_blocks - 1][bucket_index];
            failure_count += self.confirm_blocks_to_failed_txs[confirm_blocks - 1][bucket_index];
            extra_count += &self.block_unconfirmed_txs[confirm_blocks - 1][bucket_index];
            txs_count += stat.txs_count;
            // we have enough data
            while txs_count as usize >= required_samples {
                let confirm_rate = confirmed_txs / (txs_count + failure_count + extra_count as f64);
                // satisfied required_confirm_rate, find the best buckets range
                if confirm_rate >= required_confirm_rate {
                    best_bucket_start = start_bucket_index;
                    best_bucket_end = bucket_index;
                    find_best = true;
                    break;
                } else {
                    // remove sample data of the first bucket in the range, then retry
                    let stat = &self.bucket_stats[start_bucket_index];
                    confirmed_txs -= self.confirm_blocks_to_confirmed_txs[confirm_blocks - 1]
                        [start_bucket_index];
                    failure_count -=
                        self.confirm_blocks_to_failed_txs[confirm_blocks - 1][start_bucket_index];
                    extra_count -=
                        &self.block_unconfirmed_txs[confirm_blocks - 1][start_bucket_index];
                    txs_count -= stat.txs_count;
                    start_bucket_index += 1;
                    continue;
                }
            }

            // end loop if we found the best buckets
            if find_best {
                break;
            }
        }

        if find_best {
            let best_range_txs_count: f64 = self.bucket_stats[best_bucket_start..=best_bucket_end]
                .iter()
                .map(|b| b.txs_count)
                .sum();

            // find median bucket
            if best_range_txs_count != 0f64 {
                let mut half_count = best_range_txs_count / 2f64;
                for bucket in &self.bucket_stats[best_bucket_start..=best_bucket_end] {
                    // find the median bucket
                    if bucket.txs_count >= half_count {
                        return bucket
                            .avg_fee_rate()
                            .map(|fee_rate| cmp::max(fee_rate, self.min_fee_rate));
                    } else {
                        half_count -= bucket.txs_count;
                    }
                }
            }
            log::trace!("no best fee rate");
        } else {
            log::trace!("no best bucket");
        }

        None
    }
}

impl Estimator {
    /// Creates a new estimator.
    fn new(
        max_confirm_blocks: usize,
        min_bucket_feerate: f64,
        max_bucket_feerate: f64,
        fee_spacing: f64,
        decay_factor: f64,
    ) -> Self {
        let mut buckets = Vec::new();
        let mut bucket_fee_boundary = min_bucket_feerate;
        // initialize fee_rate buckets
        while bucket_fee_boundary <= max_bucket_feerate {
            buckets.push(FeeRate::from_u64(bucket_fee_boundary as u64));
            bucket_fee_boundary *= fee_spacing;
        }
        Estimator {
            best_height: 0,
            start_height: 0,
            tx_confirm_stat: TxConfirmStat::new(&buckets, max_confirm_blocks, decay_factor),
            tracked_txs: Default::default(),
        }
    }

    fn process_block_tx(&mut self, height: u64, tx_hash: &Byte32) -> bool {
        if let Some(tx) = self.drop_tx_inner(tx_hash, false) {
            let blocks_to_confirm = height.saturating_sub(tx.height) as usize;
            self.tx_confirm_stat
                .add_confirmed_tx(blocks_to_confirm, tx.fee_rate);
            true
        } else {
            // tx is not tracked
            false
        }
    }

    /// process new block
    /// record confirm blocks for txs which we tracked before.
    fn process_block(&mut self, height: u64, txs: impl Iterator<Item = Byte32>) {
        // For simpfy, we assume chain reorg will not effect tx fee.
        if height <= self.best_height {
            return;
        }
        self.best_height = height;
        // update tx confirm stat
        self.tx_confirm_stat.move_track_window(height);
        self.tx_confirm_stat.decay();
        let processed_txs = txs.filter(|tx| self.process_block_tx(height, tx)).count();
        if self.start_height == 0 && processed_txs > 0 {
            // start record
            self.start_height = self.best_height;
            log::debug!("start recording at {}", self.start_height);
        }
    }

    /// track a tx that entered txpool
    fn track_tx(&mut self, tx_hash: Byte32, fee_rate: FeeRate, height: u64) {
        if self.tracked_txs.contains_key(&tx_hash) {
            // already in track
            return;
        }
        if height != self.best_height {
            // ignore wrong height txs
            return;
        }
        if let Some(bucket_index) = self.tx_confirm_stat.add_unconfirmed_tx(height, fee_rate) {
            self.tracked_txs.insert(
                tx_hash,
                TxRecord {
                    height,
                    bucket_index,
                    fee_rate,
                },
            );
        }
    }

    fn drop_tx_inner(&mut self, tx_hash: &Byte32, count_failure: bool) -> Option<TxRecord> {
        self.tracked_txs.remove(tx_hash).map(|tx_record| {
            self.tx_confirm_stat.remove_unconfirmed_tx(
                tx_record.height,
                self.best_height,
                tx_record.bucket_index,
                count_failure,
            );
            tx_record
        })
    }

    /// tx removed from txpool
    fn drop_tx(&mut self, tx_hash: &Byte32) -> bool {
        self.drop_tx_inner(tx_hash, true).is_some()
    }

    /// estimate a fee rate for confirm target
    fn estimate(
        &self,
        expect_confirm_blocks: usize,
        min_estimate_samples: usize,
        min_estimate_confirm_rate: f64,
    ) -> Option<FeeRate> {
        self.tx_confirm_stat.estimate_median(
            expect_confirm_blocks,
            min_estimate_samples,
            min_estimate_confirm_rate,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_estimate_median() {
        let mut bucket_fee_rate = 1000;
        let bucket_end_fee_rate = 5000;
        let rate = 1.1f64;
        // decay = exp(ln(0.5) / 100), so decay.pow(100) =~ 0.5
        let decay = 0.993f64;
        let max_confirm_blocks = 1000;
        // prepare fee rate buckets
        let mut buckets = vec![];
        while bucket_fee_rate < bucket_end_fee_rate {
            buckets.push(FeeRate::from_u64(bucket_fee_rate));
            bucket_fee_rate = (rate * bucket_fee_rate as f64) as u64;
        }
        let mut stat = TxConfirmStat::new(&buckets, max_confirm_blocks, decay);
        // txs data
        let fee_rate_and_confirms = vec![
            (2500, 5),
            (3000, 5),
            (3500, 5),
            (1500, 10),
            (2000, 10),
            (2100, 10),
            (2200, 10),
            (1200, 15),
            (1000, 15),
        ];
        for (fee_rate, blocks_to_confirm) in fee_rate_and_confirms {
            stat.add_confirmed_tx(blocks_to_confirm, FeeRate::from_u64(fee_rate));
        }
        // test basic median fee rate
        assert_eq!(
            stat.estimate_median(5, 3, 1f64),
            Some(FeeRate::from_u64(3000))
        );
        // test different required samples
        assert_eq!(
            stat.estimate_median(10, 1, 1f64),
            Some(FeeRate::from_u64(1500))
        );
        assert_eq!(
            stat.estimate_median(10, 3, 1f64),
            Some(FeeRate::from_u64(2050))
        );
        assert_eq!(
            stat.estimate_median(10, 4, 1f64),
            Some(FeeRate::from_u64(2050))
        );
        assert_eq!(
            stat.estimate_median(15, 2, 1f64),
            Some(FeeRate::from_u64(1000))
        );
        assert_eq!(
            stat.estimate_median(15, 3, 1f64),
            Some(FeeRate::from_u64(1200))
        );
        // test return zero if confirm_blocks or required_samples is zero
        assert_eq!(stat.estimate_median(0, 4, 1f64), None);
        assert_eq!(stat.estimate_median(15, 0, 1f64), None);
        assert_eq!(stat.estimate_median(0, 3, 1f64), None);
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
struct Params {
    probability: f32,
    target_blocks: u32,
}

/// Confirmation Fraction Fee Estimator
///
/// Copy from https://github.com/nervosnetwork/ckb/tree/v0.39.1/util/fee-estimator
/// Ref: https://github.com/nervosnetwork/ckb/pull/1659
pub(in crate::estimators) struct FeeEstimator {
    min_samples: usize,
    kernel: Estimator,
    validator: Validator<u64>,
    statistics: Arc<RwLock<Statistics>>,
}

impl FeeEstimator {
    pub(in crate::estimators) fn new_controller(
        max_confirm_blocks: usize,
        min_samples: usize,
        min_bucket_feerate: f64,
        max_bucket_feerate: f64,
        fee_spacing: f64,
        rt: &Runtime,
        stats: &Arc<RwLock<Statistics>>,
    ) -> super::Controller {
        // half life each 100 blocks, math.exp(math.log(0.5) / 100)
        let default_decay_factor: f64 = (0.5f64.ln() / 100.0).exp();
        let kernel = Estimator::new(
            max_confirm_blocks,
            min_bucket_feerate,
            max_bucket_feerate,
            fee_spacing,
            default_decay_factor,
        );
        Self {
            min_samples,
            kernel,
            validator: Validator::new(NAME),
            statistics: Arc::clone(stats),
        }
        .spawn(rt)
    }

    fn spawn(self, rt: &Runtime) -> super::Controller {
        let (sender, mut receiver) =
            mpsc::channel::<(super::Params, Option<SyncSender<super::Result>>)>(100);
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
        super::Controller { name: NAME, sender }
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
            super::Params::RejectTransaction(tx) => {
                self.reject_transaction(&tx);
                super::Result::NoReturn
            }
        }
    }
}

impl FeeEstimator {
    fn check_estimate_params(&self, params: &Params) -> RpcResult<()> {
        let max_confirm_blocks = self.kernel.tx_confirm_stat.max_confirms();
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
        if params.target_blocks < 3 {
            return Err(RpcError::invalid_params(
                "target blocks should not less than 3 blocks",
            ));
        }
        if params.target_blocks as usize > max_confirm_blocks {
            return Err(RpcError::invalid_params(format!(
                "target blocks should not greater than {}",
                max_confirm_blocks
            )));
        }
        Ok(())
    }

    fn estimate(&self, params: &Params) -> Option<u64> {
        self.kernel
            .estimate(
                params.target_blocks as usize,
                self.min_samples,
                f64::from(params.probability),
            )
            .map(FeeRate::as_u64)
    }
}

impl FeeEstimator {
    fn submit_transaction(&mut self, tx: &types::Transaction) {
        let vbytes = patches::get_transaction_virtual_bytes(tx.size() as usize, tx.cycles());
        let fee_rate = FeeRate::calculate(Capacity::shannons(tx.fee()), vbytes as usize);
        let current_number = self.statistics.read().current_number();
        self.kernel.track_tx(tx.hash(), fee_rate, current_number);
        {
            let mut blocks_opt: Option<u64> = None;
            let probability = 0.9;
            for target_blocks in Validator::<u64>::target_blocks() {
                let params = Params {
                    probability,
                    target_blocks: *target_blocks,
                };
                let result = self.estimate(&params);
                if let Some(fee_rate_tmp) = result {
                    if fee_rate >= FeeRate::from_u64(fee_rate_tmp) {
                        blocks_opt = Some(u64::from(*target_blocks));
                        break;
                    }
                }
            }
            if let Some(blocks) = blocks_opt {
                let expected = current_number + blocks;
                log::trace!(
                    "new-tx: tx {:#x} has {:.2}% probability commit in {} blocks (before block#{})",
                    tx.hash(),
                    probability * 100.0,
                    blocks,
                    expected
                );
                let current_dt = tx.seen_dt();
                self.validator.predict(tx.hash(), current_dt, expected);
            } else {
                log::trace!("new-tx: no suitable fee rate");
            }
        }
    }

    fn commit_block(&mut self, block: &types::Block) {
        self.kernel.process_block(
            block.number(),
            block.tx_hashes().iter().map(ToOwned::to_owned),
        );
        let current_number = self.statistics.read().current_number();
        self.validator.expire(current_number);
        self.validator.confirm(block);
        self.validator.trace_score();
    }

    fn reject_transaction(&mut self, tx: &types::RejectedTransaction) {
        if tx.is_invalid() {
            self.kernel.drop_tx(&tx.hash());
        }
        self.validator.reject(tx);
    }
}

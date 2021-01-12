//! TODO CKB should expose the follow constants, structs, enumerated types and methods.

pub(crate) const DEFAULT_BYTES_PER_CYCLES: f64 = 0.000_170_571_4_f64;

const TWO_IN_TWO_OUT_CYCLES: u64 = 3_500_000;
const TWO_IN_TWO_OUT_BYTES: u64 = 597;
const TWO_IN_TWO_OUT_COUNT: u64 = 1_000;
const MAX_BLOCK_BYTES: u64 = TWO_IN_TWO_OUT_BYTES * TWO_IN_TWO_OUT_COUNT;
const MAX_BLOCK_CYCLES: u64 = TWO_IN_TWO_OUT_CYCLES * TWO_IN_TWO_OUT_COUNT;
pub(crate) const MAX_BLOCK_WEIGHT: u64 =
    MAX_BLOCK_BYTES + (MAX_BLOCK_CYCLES as f64 * DEFAULT_BYTES_PER_CYCLES) as u64;

pub(crate) fn get_transaction_virtual_bytes(tx_size: usize, cycles: u64) -> u64 {
    std::cmp::max(
        tx_size as u64,
        (cycles as f64 * DEFAULT_BYTES_PER_CYCLES) as u64,
    )
}

# [Experimental] CKB Extension: Fee Estimator

[![License]](#license)
[![GitHub Actions]](https://github.com/yangby-cryptape/ckb-extension-fee-estimator/actions)
[![Crate Badge]](https://crates.io/crates/ckb-extension-fee-estimator)
[![Crate Doc]](https://docs.rs/ckb-extension-fee-estimator)

CKB extension to estimate transaction fees.

[License]: https://img.shields.io/badge/License-MIT-blue.svg
[GitHub Actions]: https://github.com/yangby-cryptape/ckb-extension-fee-estimator/workflows/CI/badge.svg
[Crate Badge]: https://img.shields.io/crates/v/ckb-extension-fee-estimator.svg
[Crate Doc]: https://docs.rs/ckb-extension-fee-estimator/badge.svg

## :warning: Warning

**Not Production Ready!**

## Usage

- Compile:

  ```bash
  cargo build --release
  ```

- Run a CKB node.

- Run the fee estimator service:

   ```bash
   RUST_LOG="info,ckb_fee_estimator=trace" \
       ./target/release/ckb-fee-estimator \
           --subscribe-addr "${CKB_RPC_TCP_ADDRESS}" \
           --listen-addr "localhost:8080"
   ```

- Waiting for collecting enough data.

- Query via HTTP JSON-RPC:

   ```bash
   curl -H 'content-type: application/json' \
       -d '{"id":2,"jsonrpc":"2.0","method":"estimate_fee_rate","params":[{"algorithm":"vbytes-flow","probability":0.90,"target_minutes":10}]}' \
       "http://localhost:8080"
   curl -H 'content-type: application/json' \
       -d '{"id":2,"jsonrpc":"2.0","method":"estimate_fee_rate","params":[{"algorithm":"confirmation-fraction","probability":0.90,"target_blocks":100}]}' \
       "http://localhost:8080"
   ```

## JSON-RPC Methods

### `estimate_fee_rate`

- Parameters:

  - `algorithm`: The algorithm which used for estimating fee rate.

    Currently, there are two algorithms `vbytes-flow` and `confirmation-fraction`.

  - Algorithm-related parameters:

    - For `vbytes-flow` algorithm, the follow parameters should be provided:
      - `probability`: a 32-bit floating point.
      - `target_minutes`: a 32-bit unsigned integer.

    - For `confirmation-fraction` algorithm, the follow parameters should be provided:
      - `probability`: a 32-bit floating point.
      - `target_blocks`: a 32-bit unsigned integer.

- Returns:

  - Fee rate (a 64-bit unsigned integer) or null.

    With the returned fee rate, the probability of the transaction to be committed in `target_minutes` or `target_blocks` should be equal or greater than `probability`.

## Algorithms

### `vbytes-flow`

Follow the [Weight-Units Flow Fee Estimator for Bitcoin](https://bitcoiner.live/?tab=info).

### `confirmation-fraction`

More details could be found in [CKB PR 1659](https://github.com/nervosnetwork/ckb/pull/1659).

## Known Issues

- The `confirmation-fraction` algorithm always causes panics.

  I just copied the code from [CKB v0.39.1](https://github.com/nervosnetwork/ckb/tree/v0.39.1/util/fee-estimator) and didn't debug it.

  Please ignore the error logs of the `confirmation-fraction` algorithm.

## License

Licensed under [MIT License].

[MIT License]: LICENSE

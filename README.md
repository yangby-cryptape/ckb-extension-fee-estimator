# [Experimental] CKB Extension: Fee Estimator

[![License]](#license)
[![GitHub Actions]](https://github.com/yangby-cryptape/ckb-extension-fee-estimator/actions)

CKB extension to estimate transaction fees.

[License]: https://img.shields.io/badge/License-MIT-blue.svg
[GitHub Actions]: https://github.com/yangby-cryptape/ckb-extension-fee-estimator/workflows/CI/badge.svg

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
       -d '{"id": 2,"jsonrpc": "2.0","method": "estimate_fee_rate","params": [{"algorithm":"vbytes-flow", "probability":0.90, "target_minutes": 10}]}' \
       "http://localhost:8080"
   ```

## JSON-RPC Methods

### `estimate_fee_rate`

- Parameters:

  - `algorithm`: The algorithm which used for estimating fee rate.

    Currently, there is only one algorithm `vbytes-flow`.

  - Algorithm-related parameters:

    - For `vbytes-flow` algorithm, the `probability` (a 32-bit floating point) and `target_minutes` (a 32-bit unsigned integer) should be provided.

- Returns

  - Fee rate (a 64-bit unsigned integer) or null.

    With the returned fee rate,  the probability of the transaction to be committed in `target_minutes` should be equal or greater than `probability`.

## Algorithms

### `vbytes-flow`

Follow the [Weight-Units Flow Fee Estimator for Bitcoin](https://bitcoiner.live/?tab=info)

## License

Licensed under [MIT License].

[MIT License]: LICENSE

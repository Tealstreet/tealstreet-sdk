# Accepted string-literal declarations

The focused runner retains 12 concrete `v3_contracts` enum decoders against
`packages/safe-cex/src/exchanges/woo/woo.v3.contracts.ts`. Their 60 valid tags
and 252 cases remain unchanged: owned, borrowed, qualified and raw-text decoding,
positive strings, forbidden tagged-null maps, invalid kinds and padded tags.

The account/private-order/write enum owners are retired with their unused caller
closure. See ACCOUNT_WRITE_REVIEW.md. No surviving contract or native observation
changes, and no V3 migration gate is enabled.

The runner retains full compiler discovery, actual Cargo artifacts, authenticated
macro origins, selected-source scanning and refusal of omitted entrypoints.
The copied-source control still removes every selected string guard and requires
a named native mismatch; actual production source bytes must remain unchanged.

Run from `sdk-rs`, keeping rustdoc artifacts in a separate cache:

```sh
CARGO_INCREMENTAL=0 RUSTC_BOOTSTRAP=1 CARGO_TARGET_DIR=target/accepted-inventory node safe-cex-venue-woo/scripts/accepted-literal-contracts.mjs
CARGO_INCREMENTAL=0 RUSTC_BOOTSTRAP=1 CARGO_TARGET_DIR=target/accepted-inventory node safe-cex-venue-woo/scripts/accepted-literal-contracts.mjs --control
```

The original 32-owner native/wasm and compiled-control receipts describe the
pre-retirement cohort. They are not relabeled as new 12-owner executions.

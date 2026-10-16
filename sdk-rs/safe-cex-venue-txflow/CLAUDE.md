# TxFlow port

Behavioral reference: [TypeScript venue guidance](../../packages/safe-cex/src/exchanges/txflow/CLAUDE.md).

The source is a Phase 0 scaffold and so is this crate: the three URL/message
constants, `create_api` (testnet-or-mainnet `baseURL` plus a JSON
`Content-Type` over the shared `safe_cex_axios_create`), and `TxFlowExchange`,
whose only capability is a `start` that fails closed through
`BaseExchange::reject_preflight`. Mainnet is offline per the TxFlow team; do not
widen this crate past what the TypeScript implements.

`start` is the one declared divergence: the source throws synchronously, Rust
emits at invocation and returns an already-failed future. The manifest row
carries the reason.

`tests/oracle.cjs` requires `SAFE_CEX_TYPESCRIPT_PATH` and executes the actual
txflow sources with a stubbed axios factory and `BaseExchange`;
`tests/parity.rs` compares constants, factory config for testnet
true/false/absent, constructor order and the start emission. Scope checks to
this crate plus strict host and wasm clippy.

The accepted-set census classifies `TxFlowExchange::new` as typed construction.
Its contract pins the exact compiler API and its assembly of typed options,
Store/runtime/HTTP handles and the transport registry. The native witness
`tests/parity.rs::actual_typescript_constants_factory_and_start` executes that
constructor and observes its identity and eager single Phase-0 refusal, alongside
actual-source API factory comparisons. Verify the selected native/default census
owner with `node safe-cex-parity/scripts/accepted-set.mjs --package=safe-cex-venue-txflow --jobs=1` from `sdk-rs`.

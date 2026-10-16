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

The opt-in Rust registry path constructs this same Phase-0 scaffold through
`safe-cex-registry::rust_venues::txflow_constructor`. It does not widen venue
support. The native registry witness observes eager rejection and no HTTP/WS IO
for both network settings; `scripts/registry-control.py` swaps the factory to a
compiling wrong venue and requires that witness to fail, then restores bytes.

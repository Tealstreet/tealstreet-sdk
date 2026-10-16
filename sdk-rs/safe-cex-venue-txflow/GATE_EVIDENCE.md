# Phase-0 native registry witness

The opt-in Rust constructor table now constructs the existing `TxFlowExchange`
scaffold. The source `txflow.exchange.ts` only implements a refusing `start`;
this registry entry adds no venue REST, WebSocket or trading support.

`safe-cex-registry/tests/native_host_txflow.rs` constructs with production native
inputs for testnet false/true, observes the single error before polling start,
asserts the exact Phase-0 refusal, and verifies no HTTP requests, socket dials or
market writes. `tests/parity.rs::actual_typescript_constants_factory_and_start`
executes the actual TypeScript to pin the existing constants/factory/start.

From `sdk-rs`, `python3 safe-cex-venue-txflow/scripts/registry-control.py` swaps
only the table's TxFlow factory to the compiling Backpack factory. The native
witness fails with exit101; exact source restoration passes with exit0. Both
outcomes were measured on 2026-10-17, alongside the native witness and all13
constructor tests. Strict registry host all-target and wasm library Clippy,
canonical parity, the deliberate layer list and live registry equivalence are
separate integration gates.

# TxFlow replay reference

Test-only TypeScript testnet adapter retained for Rust replay comparison and
independent audit tests. Production registries use `RustEngineExchange`.
Keep the reference out of production imports. Protocol and supported-feature
contracts live in `src/exchanges/txflow/CLAUDE.md`; the harness compares output
directly, without TxFlow-specific normalization.

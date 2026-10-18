# WOO websocket state review

The V3 acknowledgement and subscription helpers remain inert. Their existing
`ws_state` target retains the eight blocker records, command-ID metadata,
31 acknowledgement cases, 12 create cases, 12 plan cases, nine acknowledgement
transitions and six reconnect cases, with native fields and number bits checked
before any wire projection. Its oracle executes the maintained TypeScript module.
The three remaining compiled controls cover blocker parsing, acknowledgement time
and reconnect generation; the retired sequence control is removed.

## Sequence retirement

Assigned FIX-audit row 46 is ACCEPT because no production TypeScript caller imports
`advanceWooV3OrderBookSequence`. Repository references are its declaration at
`packages/safe-cex/src/exchanges/woo/woo.v3.websocket.ts:774` and the audit test
`packages/safe-cex/src/__tests__/audit-regressions/woo.v3-websocket-contracts.test.ts`.
The maintained constructed-class dependency walk loads no V3 module, and the real
native exchange exposes no sequence member or global.

The only Rust callers were `tests/ws_state.rs`. The isolated function, its result
enum, two orphan literal codecs, nine facade cases, declaration/strict-result
checks and sequence-only control are removed together. Acknowledgement and
subscription inputs, assertions and source observations remain unchanged. The
obsolete historical census and generated gate report are removed as well.

Scoped validation uses `fix_audit_tests::`, `--test ws_state` and
`binding_streams::listen_order_book_joins_the_snapshot_with_buffered_deltas`.
The latter constructs the real exchange and exercises signed snapshot HTTP,
public socket deltas, orderbook publication and cleanup before and after removal.
All-target strict Clippy compiles the remaining consumers. Full parity indexing
keeps both TypeScript rows as not-applicable and refuses stale Rust targets.
This is dead-code retirement, not a claim of a master-red behavior repair.

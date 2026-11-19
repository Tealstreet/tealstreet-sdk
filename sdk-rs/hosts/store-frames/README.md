# Native UI frame production

This host boundary composes engine models and native chaser rows into UI records.
It does not depend on the restored port crates. Financial calculations use native
floating-point arithmetic and display labels use Rust fixed precision formatting.
Generated TypeScript records remain unchanged.

SCXF sections 26–32 replace socket health, active chasers, active TWAPs, order
risk, displayed positions, displayed margin and balance info. Section 25 remains
the engine's subscription diagnostics table. Section 33 replaces native managed-order ownership by exact order ID; empty maps clear chaser/TWAP tags. Common engine sections 34 and 35 carry shared public subscription diagnostics and monotonic request-outcome counters respectively. Absent sections preserve consumer
identities; empty collections clear them, and balance info null clears that field.

The typed Serde encoder lives in `engine/src/store_frame_wire.rs`. Revision one
uses length-prefixed UTF-8 strings and binary64 numbers, written directly from
native records, with no intermediate JSON conversion. A payload starts with a
u32 byte length and ends with zero padding to eight bytes. Tags 1–7 are null,
false, true, number, string, array and record; collection counts are u32. Record
keys are length-prefixed strings. The pure-JS reader also accepts the previous
revision zero during client upgrades. Generated TypeScript types are unchanged.

Only opaque balance info and existing algorithm metadata use host JSON values;
position, market, ticker, balance and risk calculations use engine records.
Holder-label projection copies unrelated bytes, preserving floating-point bits.
Snapshots do not consume the streaming publication baseline. Router fencing
flushes the final paused rows before retiring the actor. PnL history is bounded
and sampled at most once per second. Delisted TWAPs retain their admitted market.

Run scoped validation from `sdk-rs/`:

```sh
cargo test --locked -p safe-cex-store-frames
cargo clippy --locked -p safe-cex-store-frames --all-targets
node --test hosts/wasm/js/wire.test.mjs
PATH=/tmp/pybin:$PATH ./scripts/check-bans.sh
```

The risk differential test needs the root Yarn TypeScript dependency; set
`SAFE_CEX_TYPESCRIPT_PATH` if it is outside normal Node resolution. Its numeric
comparison permits floating-point roundoff from native arithmetic.

Display PnL uses the shared `packages/safe-cex/src/exchange-display-policy.json`
metadata plus `tuning.calculateUpnlFromRealtimeLastPrice`. Both hosted and LOCAL
constructors pass this tuning into the producer; relabeling preserves it. The
policy is compiled at build time and included in wasm artifact fingerprints.
Bybit's default and explicit tuning both calculate `(110 - 100) * 2 = 20`,
even when the venue reports 7. The source position retains the venue value.

Native SETTINGS completions update canonical position leverage rails. FrameStore
retains position settings after closure and recovery snapshots, and owns its
monotonic row update counters. Readiness requires decoded catalogue, ticker and
account evidence in addition to STATUS claims. A STATUS-only snapshot is unloaded.

Shared public diagnostics enter the account engine as validated public-feed
facts, with independent replacement and retained message ages. They are emitted
in account deltas and late snapshots, so forwarding workers use the same path.
In-process public sources must expose their native `snapshotFrame`; the native
shared-feed server refuses a source without it. Lighter limits use native
validated API metadata in `balance.info.lighterAccountLimits`.

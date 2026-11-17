# Native UI frame production

`safe-cex-store-frames` is the shared host boundary for the hosted account actor
and LOCAL wasm engine. The venue engine retains its small internal models;
this crate constructs the public rows from `safe-cex-types`, the same records
which generate the TypeScript API. There is no mutable TypeScript store or
`op:'store'` transport in this path.

SCXF sections 26–32 replace socket health, active chasers, active TWAPs, order
risk, displayed positions, displayed margin and balance info, respectively.
The codec is `safe-cex-types/src/store_frame_wire.rs`; `hosts/wasm/js/wire.mjs`
is the independent, pure-JS reader used by web and Hermes. Each section has a
little-endian u32 payload length, tagged native values, then zero padding to an
eight-byte boundary. Tags 0–7 mean undefined, null, false, true, binary64,
UTF-16 text, array and object. Text/array/object counts are u32. Object keys are
UTF-16 text without a tag. Sections are revision zero. An absent section leaves
the consumer's current value intact; present empty collections clear it.
`balanceInfo()` returns null to clear previously published info.

The existing section 25 carries native subscription diagnostics. Its reader
returns `replace` plus the public/business/futures/private topic rows and their
monotonic ages. It requires no derived store message.

Derived projections cache their generated rows and encoded sections. Only
changed sections are appended. Position PnL windows sample at most once per
second and retain one/five minutes of history; closed position keys are evicted.
Socket flags and close diagnostics publish immediately, timestamp-only changes
at most once per five seconds. Snapshots carry every section without consuming
the streaming publication baseline. Router fencing flushes its final paused
rows before retiring the actor. Holder-specific account labels are rewritten
inside the native TWAP/position sections while all other sections keep their
original bytes.

The TypeScript risk policy and real safe-math helper are executed independently
against the native result in the scoped tests. As in the existing shared-type
oracles, install root Yarn dependencies and set `SAFE_CEX_TYPESCRIPT_PATH` when
TypeScript is outside this checkout's module resolution:

```sh
SAFE_CEX_TYPESCRIPT_PATH=/path/to/node_modules/typescript cargo test --locked -p safe-cex-store-frames
cargo clippy --locked -p safe-cex-store-frames --all-targets
node --test hosts/wasm/js/wire.test.mjs
```

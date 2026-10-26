# Non-UTF-8 FIX-audit rulings

The assignment is the `safe-cex-types` pair in the frozen
`non-utf8-fix-by-crate.json` census. These verdicts apply to the named type
boundaries, not every provider data decoder or subscription capability.

| Source row | Verdict | Binding evidence and reason |
| --- | --- | --- |
| `external-public-data.ts::GatewayChannel` | ACCEPT | The TS union omits `auth`, but the actual provider sends `subscribe/auth` with the configured token. Rust's `Auth` variant preserves that frame. The native registry constructor, NativeRuntime and loopback WebSocket deliver the exact auth payload before the market/ticker subscriptions; the maintained TS provider sends the same frame and accepts its acknowledgement. Widening a type to include a value the source already emits does not change production behavior. |
| `external-public-data.ts::GatewayMessage` | ACCEPT | The public Rust envelope decoder is stricter than the unchecked TS cast, but production gateway ingress does not invoke it. The real constructed Binance exchange starts over the native gateway with market/ticker frames containing null `symbol`, object `interval`, numeric `error`, boolean `requestId` and an unknown top-level member. Both rows reach Store with their original values; no venue REST fallback runs. The test separately proves that the standalone envelope decoder rejects those same frames. Unknown type/channel tags do not prevent the subsequent valid updates. The envelope is used to construct outbound frames, whose supplied fields are valid. Strict decoding remains an explicit API boundary for callers who choose that type; it is not a whole-frame production refusal. |

The native witness is
`safe-cex-registry/tests/native_host_external_data.rs::gateway_type_audit_uses_channel_fields_without_envelope_validation`.
Its source comparison executes the maintained `ExternalPublicDataProvider` in
`safe-cex-registry/tests/public-data.cjs`. Assertions cover actual native Store
values and outbound auth bytes before comparing the source summary.

Two independent compiled controls validate the witness. Requiring
`GatewayMessage` decoding before raw ingress fails the bounded native startup
assertion. Replacing the outbound `Auth` channel with `Ticker` fails the exact
native auth-frame assertion. Each fault was restored byte-for-byte and the same
native test passed again. Neither fault is part of the production change.

The witness deliberately does not claim arbitrary extra fields inside ticker or
market rows, unknown candle intervals, JSON projection of opaque data, or
callback timing. Those are separate boundaries with their own audit rows.

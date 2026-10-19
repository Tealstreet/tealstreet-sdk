# Native input owner review

This batch closes the six Aster entries in `new-native-input-owners.json`.
Five ordinary decoder contracts execute 40 native observations; `binding::scope`
is an explicit non-decoder context capability. The canonical scoped measurement
retains full compiler discovery and authenticates each actual declaration.

| Owner | Ruling | Reached production evidence |
| --- | --- | --- |
| `binding::globals::bytes` | ACCEPT internal byte representation | Constructed signing globals compare empty and nonempty byte sequences, keccak and Ethereum message hashes with ethers. The V3 signed-client test exercises the actual ABI read/write signer. Invalid caller-forged elements are outside these internal producers. |
| `binding::globals::hex_bytes` | ACCEPT closed computed-hash domain | The source `ethers.getBytes(payloadHash)` receives the computed keccak hash. Installed native getBytes matches ethers for valid hashes; standalone contracts also expose malformed-hex errors without claiming their error classes match ethers outside that domain. |
| `binding::scope` | ACCEPT typed context capability | Actual constructed venue retains receiver identity and the same shared owner; an expired Weak rejects visibly. A compiled receiver-discard control must fail and exact restoration must pass. No decoder or identity-fallback classification is used. |
| `binding::text` | ACCEPT approved UTF-8 view | Installed HMAC signer executes the maintained TS `generateV1Signature` body and compares ASCII/Unicode signatures. Throwing primitive conversion preserves its original error. Standalone cases expose signed zero, optional values and the typed lone-unit replacement. |
| `private_stream::utf8` | FIX raw fill symbol loss | A real constructed private socket frame previously emitted undefined for numeric symbol 21. `PrivateFill.symbol` now captures canonical RetainedJsonInput at the copy site; Parent materializes it directly. Ten actual-source/socket cases cover number, boolean, null, object, array, ordinary/lone-unit strings, negative zero, infinity and missing symbol. |
| `exchange_helpers::transform_klines` | ACCEPT typed Candle boundary | Existing constructed V1/V3 REST tests execute source fetchOHLCV against short, numeric and indexed rows and unused columns. Native observations read all six raw fields and bits; the additional production-helper test retains custom map receiver and original row-getter rejection. |

The raw fill is an event value, so it cannot use the closed typed model boundary
to erase a value. The fix changes only this venue-local event carrier, without
widening Order, Position or Candle. Owned object snapshots retain the established
decision-2 identity limit. Remaining utf8 calls implement strict status dispatch
or typed Order/Position symbol views; this batch does not claim a lossless raw
model graph at those closed output boundaries.

`native_emitter_tests` invokes `accepted-native-input-source.cjs` in normal Cargo
tests. The source probe executes extracted maintained private handler/reconcile
bodies and the actual HMAC function with primitive timer/crypto dependencies.
It is not a replacement full exchange implementation. Rust uses the constructed
Aster exchange, NativeRuntime, reqwest host and real private socket dispatcher.
The ten source fills each schedule the existing 10-second repair timer.

The original production-body regression is recorded in
`/tmp/kxakrb-aster-native-input-fill-red.log`; its exact fixed replay is
`/tmp/kxakrb-aster-native-input-fill-green.log`. Expanded native and scope-control
receipts use the same prefix. A failed control compilation is not behavioral
proof; only the subsequent compiled named assertion failure is counted.

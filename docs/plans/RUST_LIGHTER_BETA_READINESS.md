# Lighter beta readiness

Status: reviewer-CLEAR public-data slice on the managed funnel in #2230. Grouped limit
TP/SL is owned by kuhwxi as a separate slice; this slice does not enable it.

The beta runs through `hosts/wasm/js/host.mjs`. A live public recording on #2230
captured four HTTP replies and 15,401 socket frames, but replay found dropped
stats rows and book aggregation differences. Native tests alone are not beta
qualification. The original tape and failing report remain in scratch.

## Public data corrections

- Decode base and USD open interest separately. Prefer base open interest when
  both exist, regardless of JSON order, so the rest of the stats row survives.
- Seed last price from the accepted market snapshot and start ticker hydration
  immediately, before socket Open. The existing single-flight guard prevents
  duplicate reads if the socket opens while that read is pending.
- Aggregate REST depth at the market's native amount precision, without binary
  addition residue. Deltas replace a complete parsed native size, without
  addition. Preserve price ordering, full depth and zero handling.
- On public close, request one prompt managed REST snapshot for each subscribed
  book. Keep the existing single-flight and pacing rules. An older REST reply
  cannot overwrite a newer socket snapshot. Repeated closes cannot multiply
  in-flight requests; unsubscribed books remain quiet.

The first four added public tests run on unmodified production source:
13 native controls pass and four intended assertions fail. Additive source
controls will cover key order, optional values, reconnect flapping and a fresh
socket snapshot beating the recovery read. Do not relax replay tolerances or
filter the captured tape.

## JS/WASM qualification

The composed module at 59c0e290000 (tree 399218431955) is 1,255,487 bytes for
both web and Node, SHA256
ecc88700c81c26def6b69f2836d0537314ca8d3779385db0ac01d5ae903a3a5e.
The unchanged retained tape now agrees on prices, books and later checkpoints.
Its 576 remaining differences are checkpoint-zero volume, quoteVolume and
percentage: Rust publishes real exchangeStats daily values while TypeScript's
fallback drops those fields. The overseer accepts this class as Rust-better;
the readiness gate passes with this disclosure. Other differences still block.
The raw FAIL report and tape remain unchanged; comparison tolerances and filters
are unchanged. A fresh JS/WASM EngineHost public capture ran for 15 seconds with
five HTTP replies, 12,323 socket frames, two books and 60 Store updates. This is
the Node binding of the browser host implementation, not a native-host test.
Replaying that fresh capture reports exactly the same 576 accepted bootstrap
counter differences and zero differences outside that class. Its tape SHA256 is
4fedc4a2150c0fd5460db15feb612b678232f7c3c4b7c6e9c5d350281f6934aa.
The composed native suite passes 327 tests, with strict all-target clippy; these
support regression coverage and are separate from the JS/WASM evidence above.
CI's retained public fixture reports 588 differences in the same accepted
startup-counter class, with none outside it. Its construction-test failure was
the tip-wide missing Lighter mapping, fixed separately by #2286 and composed
here without changing Lighter runtime or assertions.

The accepted CI bootstrap differences are now exact tape-and-path declarations
with a TS-zero/finite-Rust value guard. Only checkpoint-zero volume, quoteVolume
and percentage qualify. Raw reports retain FAIL; the correctness verifier accepts
the declared improvement. Other checkpoints, fields, tapes or venues still fail.
The two private tapes retain their separate pre-existing fill/order declarations;
their eleven differences each are not part of the public bootstrap class.

## Remaining beta blocker

Grouped limit entry with TP/SL must use one type-28 OTO/OTOCO transaction and one
nonce through TxRecipe. Confirm the parent and every child before success; an
unconfirmed child means Unknown with proven IDs and uncertainty retained, with
no replay. Lock every child ID. Definitive pre-execution 21104 may reseed once;
408/5xx cannot. Invalid triggers or unsupported splits refuse before signing.
Kuhwxi owns its financial tests and review before credentialed smoke.

Reviewer: dazzy3. No engine, crypto, registry, budget or inventory edit is planned
for the public-data slice. The bounded replay-expectation follow-up also requires
an exact-tree auditor GO. The final JS/WASM build must replay the retained live
tape and pass a fresh live-public run before claiming beta readiness.
